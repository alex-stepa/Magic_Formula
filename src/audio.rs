// audio — zvuk motoru. GRANULÁRNÍ SYNTÉZA přes vlastní cpal výstup (nízká latence).
// Jeden "combustion" grain se spouští na zápalové frekvenci (rpm×válce) s překrýváním
// voices; pitch = časování spouštění (fázový akumulátor v audio-čase). Výstup jde přímo
// přes cpal s malým bufferem — obchází Bevy audio (~250ms buffer = latence + schody).
// Herní systém píše rpm/válce/gain do sdílených atomiků, cpal callback je čte.
//
// Zbytek souboru (EngineDecoder, poly_saw, LoopAudio…) je STARÝ procedurální synth /
// loop generátor — už se nepoužívá, jen offline měřicí/generovací nástroje v testech.
// Proto allow(dead_code). ponytail: kandidát na smazání, až bude granulár finální.
#![allow(dead_code)]

use bevy::audio::{Decodable, Source};
use bevy::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;

use crate::solver::SoftBody;
use crate::ui::AppState;
use crate::vehicle::DrivetrainState;

const SAMPLE_RATE: u32 = 44_100;
// Otáčky, na kterých je engine_loop.wav nahraný. set_speed = rpm/BASE → pitch. Nahradíš-li
// loop skutečnou nahrávkou, nastav sem rpm té nahrávky.
const BASE_LOOP_RPM: f32 = 2000.0;

// Sdílený stav mezi herním systémem (zapisuje) a audio vláknem (čte). f32 se nese
// jako bity v AtomicU32 — lock-free, per-sample čtení levné.
#[derive(Clone)]
struct EngineShared {
    rpm: Arc<AtomicU32>,
    gain: Arc<AtomicU32>,     // cílová lineární hlasitost (0 = ticho)
    cylinders: Arc<AtomicU32>,
    config: Arc<AtomicU32>,   // 0=Inline 1=V 2=Boxer
}

impl EngineShared {
    fn new() -> Self {
        Self {
            rpm: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            gain: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            cylinders: Arc::new(AtomicU32::new(4)),
            config: Arc::new(AtomicU32::new(0)),
        }
    }
}

// Asset = jen držák sdíleného stavu; Bevy z něj přes Decodable vyrobí decoder.
#[derive(Asset, TypePath, Clone)]
struct EngineAudio {
    shared: EngineShared,
}

impl Decodable for EngineAudio {
    type DecoderItem = f32;
    type Decoder = EngineDecoder;
    fn decoder(&self) -> EngineDecoder {
        EngineDecoder::new(self.shared.clone())
    }
}

// Nekonečný generátor vlny. phase 0..1 = jeden zápalový cyklus.
struct EngineDecoder {
    shared: EngineShared,
    phase: f32,
    freq: f32, // vyhlazená zápalová frekvence (Hz)
    gain: f32, // vyhlazená hlasitost
    phase_half: f32, // fáze na polovině zápalové frekvence (půl-řád rumble)
    lp: f32,        // stav lowpassu (tělo, nepiní pitch)
    dc: f32,        // stav DC-blocku
    rng: u32,       // xorshift stav pro šum spalování
    cycle_jit: f32, // náhodná amplituda tohoto zápalu (cycle-to-cycle variace)
}

impl EngineDecoder {
    fn new(shared: EngineShared) -> Self {
        Self { shared, phase: 0.0, freq: 0.0, gain: 0.0, phase_half: 0.0, lp: 0.0, dc: 0.0, rng: 0x9e3779b9, cycle_jit: 1.0 }
    }

    // Bílý šum -1..1 (xorshift32, levný, per-sample).
    fn noise(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

impl Iterator for EngineDecoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let target_rpm = f32::from_bits(self.shared.rpm.load(Relaxed));
        let target_gain = f32::from_bits(self.shared.gain.load(Relaxed));
        let cyl = self.shared.cylinders.load(Relaxed).max(1) as f32;
        let cfg = self.shared.config.load(Relaxed);

        // Zápalová frekvence: rpm/60 otáček/s × cyl/2 zápalů/otáčku (4-takt).
        let target_freq = (target_rpm / 120.0) * cyl;

        // rpm ze hry přijde jen 60×/s → cíl je schodovitý (krok ~1–7 Hz za snímek).
        // Klíč: filtr s časovou konstantou ~15 ms (delší než snímek 16 ms) se NIKDY nedožene
        // mezi dvěma updaty → freq se pořád plynule pohybuje, žádné držení = žádné schody.
        // Rychlejší filtr (dřív 0.08) doběhl a držel → slyšitelné diskrétní tóny.
        // ponytail: a≈0.0015 je kompromis hladkost vs. ~15 ms lag; menší = hladší/pomalejší.
        self.freq += (target_freq - self.freq) * 0.0015;
        self.gain += (target_gain - self.gain) * 0.004;

        // Fáze na ZÁPALOVÉ frekvenci → celý tón klouže s otáčkami (pitch jde po křivce,
        // ne po schodech). Detekce přetečení = nový zápalový cyklus.
        let inc = self.freq / SAMPLE_RATE as f32;
        self.phase += inc;
        self.phase_half += inc * 0.5; // půl-řád (rumble) běží na polovině
        self.phase_half -= self.phase_half.floor();
        let mut n = self.noise();
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            // Cycle-to-cycle variace amplitudy — motor není dokonale periodický (jinak
            // zní jako hudební nástroj). Náhoda jednou za zápal.
            self.cycle_jit = 0.75 + 0.25 * (n * 0.5 + 0.5);
        }

        let (decay, bright, noise_amt) = firing_params(cyl, cfg);

        // ZDROJ TÓNU = polyBLEP pila (anti-aliased): plná harmonická řada, ale BEZ aliasingu
        // a BEZ vypadávání harmonických → barva se mění spojitě, pitch klouže. Naivní pila
        // (dřív) aliasovala = inharmonické tóny co skáčou = slyšitelné „schody". Půl-řád pila
        // přidá nízké „lope"/rumble motoru. Jemná amplitudová modulace na zápalu (ne gate do
        // ticha) = pulz textura bez děr mezi ranami.
        let saw = poly_saw(self.phase, inc);
        let saw_half = poly_saw(self.phase_half, inc * 0.5);
        let tone = saw * 0.6 + saw_half * 0.4;

        // Jemná AM: pulz zápalu moduluje hlasitost 0.7..1.0 (rasp), nikdy neutne do ticha.
        let pulse = (-self.phase * decay).exp() * self.cycle_jit;
        let am = 0.7 + 0.3 * pulse;
        n = self.noise();
        let raw = (tone + n * noise_amt) * am * bright;

        // Lehký lowpass jen na jemné vyhlazení (band-limit už řeší aliasing).
        self.lp += (raw - self.lp) * 0.6;
        // DC-block, ať vlna kmitá kolem 0.
        self.dc += (self.lp - self.dc) * 0.0008;
        let out = (self.lp - self.dc) * self.gain * 1.4;

        Some(out.clamp(-1.0, 1.0))
    }
}

impl Source for EngineDecoder {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None // nekonečný stream
    }
}

// polyBLEP: korekce pily kolem nespojitosti (skok o 2 na konci cyklu) → anti-aliasing.
// t = fáze 0..1, dt = přírůstek fáze/sample. Bez ní naivní pila aliasuje = skákavé
// inharmonické tóny. Standardní 2-bodová polyBLEP aproximace.
fn poly_blep(mut t: f32, dt: f32) -> f32 {
    if t < dt {
        t /= dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

// Anti-aliased pila -1..1 z fáze (bohaté harmonické, spojitá barva, pitch klouže).
fn poly_saw(phase: f32, dt: f32) -> f32 {
    (2.0 * phase - 1.0) - poly_blep(phase, dt)
}

// Parametry zvuku podle motoru. `decay` = šířka pulzu (AM textura). `bright` = celková
// hlasitost/jas složky, `noise_amt` = množství šumu (rasp). V jiskřivější, Boxer hrubší.
// ponytail: knoby tady, ladí se sluchem.
fn firing_params(cyl: f32, cfg: u32) -> (f32, f32, f32) {
    let mut decay = 9.0 + (cyl - 4.0) * 1.2;
    let mut bright = 0.9;
    let mut noise_amt = 0.10;
    match cfg {
        1 => { decay *= 0.8; bright = 1.0; noise_amt = 0.07; } // V — jiskřivější
        2 => { decay *= 1.4; bright = 0.85; noise_amt = 0.14; } // Boxer — hrubší
        _ => {}
    }
    (decay, bright, noise_amt)
}

// Herní strana — GRANULÁRNÍ SYNTÉZA (jak to dělají hry) ----------------------
// Osvědčená technika (Wescott na comp.dsp, Wwise, racing sims): jeden "combustion" grain
// se SPOUŠTÍ na zápalové frekvenci a jednotlivá spuštění se PŘEKRÝVAJÍ (polyfonie), jak
// rostou otáčky. Pitch = rychlost spouštění, řízená fázovým akumulátorem v AUDIO-čase →
// žádný pitch/speed parametr, co by přeskakoval kvůli buffernutí atomiku (to byl bug
// loop/synth verzí: audio vlákno spočítá buffer naráz se zamrzlým rpm a smoothing doběhl +
// držel). Tady rpm ovlivňuje jen ČASOVÁNÍ spouštění → drobná chyba = neslyšitelný jitter.
// Grain se syntetizuje jednou při startu (žádný asset).

// Jeden zápal válce (~150 ms). HLUBOKÝ výfukový „boom" (nízké resonance, pomalý decay =
// tělo/basy), krátký mechanický „knock" (atak), a TMAVÝ tlumený šum (textura, ne sykot).
// Dřív to znělo jako bruska = moc vysokého šumu a krátký jasný grain. Teď těžiště dole.
fn make_combustion_grain(sr: f32) -> Vec<f32> {
    use std::f32::consts::TAU;
    let n = (sr * 0.15) as usize;
    let mut rng = 0x9e3779b9u32;
    let mut noise = move || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        (rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    let mut lpn = 0.0; // silně tlumený šum (tmavý rachot, žádný sykot)
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr;
        // Hluboký boom: nízké harmonické, pomalý decay = basová „rána" motoru.
        let boom = ((TAU * 52.0 * t).sin() * 0.9 + (TAU * 104.0 * t).sin() * 0.35 + (TAU * 156.0 * t).sin() * 0.12)
            * (-t * 13.0).exp();
        // Mechanický knock: velmi krátký transient na začátku (hrana, ne pískot).
        let knock = noise() * (-t * 450.0).exp() * 0.25;
        // Tmavý rachot: silně lowpassnutý šum, tlumený pomalou obálkou.
        lpn += (noise() - lpn) * 0.05;
        let rumble = lpn * (-t * 16.0).exp() * 0.5;
        let mut s = boom + knock + rumble;
        // atak (0.5 ms) a fade konce (20 ms) → hladké okraje, žádné kliky
        let atk = sr * 0.0005;
        if (i as f32) < atk {
            s *= i as f32 / atk;
        }
        let tail = (n - i) as f32;
        let fade = sr * 0.02;
        if tail < fade {
            s *= tail / fade;
        }
        out.push(s);
    }
    out
}

// Sdílený stav: rpm, válce (zápalová frekvence), objem (litry → velikost válce → hloubka),
// cílová hlasitost.
#[derive(Clone)]
struct GrainShared {
    rpm: Arc<AtomicU32>,
    cyl: Arc<AtomicU32>,
    disp: Arc<AtomicU32>, // objem motoru (litry, f32 v bitech)
    config: Arc<AtomicU32>, // 0=Inline 1=V 2=Boxer → nerovnoměrnost zápalů (burble)
    gain: Arc<AtomicU32>,
    // DEBUG capture: pokud Some, decoder sem tlačí svůj výstup (skutečný zvuk hry).
    rec: Option<Arc<std::sync::Mutex<Vec<f32>>>>,
}

// Jeden běžící grain (spuštěný zápal). `speed`/`amp` mírně randomizované = motor není
// dokonale periodický (jinak zní jako nástroj).
struct Voice {
    pos: f32,
    speed: f32,
    amp: f32,
}

// Granulární synth — generuje vzorky v cpal callbacku (ne přes Bevy/rodio, kvůli latenci).
struct GrainSynth {
    grain: Arc<Vec<f32>>,
    shared: GrainShared,
    sr: f32,    // sample rate výstupního zařízení (z cpal)
    phase: f32, // fázový akumulátor zápalů (0..1 = jeden zápalový interval)
    freq: f32,  // vyhlazená zápalová frekvence (Hz)
    gain: f32,  // vyhlazená hlasitost
    norm: f32,  // vyhlazená normalizace na počet překryvů (ať idle a redline drží hlasitost)
    lp: f32,    // lowpass výstupu (utne vysokou brusku, drží zvuk hluboký)
    next_thresh: f32, // práh fáze pro další zápal (nerovnoměrný podle configu = burble)
    fire_i: u32,      // počítadlo zápalů (pro střídání intervalů)
    voices: Vec<Voice>,
    rng: u32,
}

impl GrainSynth {
    fn new(grain: Arc<Vec<f32>>, shared: GrainShared, sr: f32) -> Self {
        Self { grain, shared, sr, phase: 0.0, freq: 0.0, gain: 0.0, norm: 1.0, lp: 0.0, next_thresh: 1.0, fire_i: 0, voices: Vec::with_capacity(64), rng: 0x2545f491 }
    }

    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    // Vygeneruj jeden vzorek. Volá se z cpal callbacku (malý buffer = nízká latence, jemná
    // kontrola → rpm se čte z atomiku často, žádné velké schody jako přes Bevy buffer).
    fn tick(&mut self) -> f32 {
        let glen = self.grain.len();
        if glen < 2 {
            return 0.0;
        }
        let rpm = f32::from_bits(self.shared.rpm.load(Relaxed));
        let cyl = self.shared.cyl.load(Relaxed).max(1) as f32;
        let disp = f32::from_bits(self.shared.disp.load(Relaxed)).max(0.1);
        let cfg = self.shared.config.load(Relaxed);
        let target_gain = f32::from_bits(self.shared.gain.load(Relaxed));

        // Zápalová frekvence: rpm/60 ot/s × cyl/2 zápalů/otáčku (4-takt). Víc válců = víc
        // zápalů/s = hustší, hladší zvuk (emergentně).
        let target_freq = (rpm / 120.0) * cyl;
        self.freq += (target_freq - self.freq) * 0.002; // vyhlazení jen časování
        self.gain += (target_gain - self.gain) * 0.006;

        // VELIKOST VÁLCE = objem/válce → hloubka grainu. Velký válec (velkoobjemový V8) =
        // pomalejší přehrávání grainu = hlubší rána; malý válec (1.4 i4, motorka) = vyšší.
        // Referenční 0.5 L/válec → pitch 1.0. Odmocnina = mírné škálování.
        let cyl_size = disp / cyl;
        let grain_pitch = (0.5 / cyl_size).sqrt().clamp(0.5, 1.7);

        // Fázový akumulátor → při dosažení prahu spusť nový grain (voice). Práh je NEROVNO-
        // MĚRNÝ podle uspořádání (V/Boxer střídá kratší/delší interval = charakteristický
        // burble/rumble; Inline rovnoměrně). Fyzikálně: nerovnoměrné rozestupy zápalů.
        self.phase += self.freq / self.sr;
        if self.phase >= self.next_thresh {
            self.phase -= self.next_thresh;
            self.fire_i = self.fire_i.wrapping_add(1);
            let uneven = match cfg {
                1 => 0.12, // V — burble
                2 => 0.17, // Boxer — silnější offbeat rumble
                _ => 0.0,  // Inline — rovnoměrně
            };
            self.next_thresh = if self.fire_i % 2 == 0 { 1.0 - uneven } else { 1.0 + uneven };
            if self.voices.len() < 48 {
                let speed = grain_pitch * (1.0 + self.noise() * 0.03); // pitch dle válce ±3 %
                let amp = 0.85 + 0.15 * (self.noise() * 0.5 + 0.5);
                self.voices.push(Voice { pos: 0.0, speed, amp });
            }
        }

        // Sečti aktivní voices (lineární interpolace).
        let mut sum = 0.0;
        for v in self.voices.iter_mut() {
            let i = v.pos as usize;
            if i + 1 < glen {
                let frac = v.pos - i as f32;
                sum += (self.grain[i] * (1.0 - frac) + self.grain[i + 1] * frac) * v.amp;
            }
            v.pos += v.speed;
        }
        self.voices.retain(|v| (v.pos as usize) + 1 < glen);

        // Normalizace na √(překryvy), vyhlazená (skok počtu voices by cvakal).
        let target_norm = 1.0 / (self.voices.len().max(1) as f32).sqrt();
        self.norm += (target_norm - self.norm) * 0.002;
        let raw = sum * self.norm * self.gain;
        // Lowpass výstupu (~1.5 kHz @48k) → sundá vysokou „brusku", nechá hloubku. Pak
        // jemný soft clip (menší drive = míň tvrdých harmonických).
        self.lp += (raw - self.lp) * 0.20;
        (self.lp * 1.2).tanh()
    }
}

#[derive(Resource, Clone)]
struct EngineHandle(GrainShared);

// Drží živý cpal stream (jinak by se dropnul a ztichl). !Send → NonSend resource.
struct AudioStream(#[allow(dead_code)] cpal::Stream);

pub struct EngineSoundPlugin;

impl Plugin for EngineSoundPlugin {
    fn build(&self, app: &mut App) {
        // Exkluzivní startup systém: cpal::Stream je !Send → musí přes NonSend resource.
        app.add_systems(Startup, setup_engine_audio)
            .add_systems(Update, drive_engine_sound);
    }
}

fn setup_engine_audio(world: &mut World) {
    let rec = std::env::var("MF_RPM_TRACE").is_ok().then(|| Arc::new(std::sync::Mutex::new(Vec::new())));
    let shared = GrainShared {
        rpm: Arc::new(AtomicU32::new(850.0f32.to_bits())),
        cyl: Arc::new(AtomicU32::new(4)),
        disp: Arc::new(AtomicU32::new(1.4f32.to_bits())),
        config: Arc::new(AtomicU32::new(0)),
        gain: Arc::new(AtomicU32::new(0.0f32.to_bits())),
        rec,
    };
    world.insert_resource(EngineHandle(shared.clone()));
    match build_cpal_stream(shared) {
        Some(stream) => world.insert_non_send_resource(AudioStream(stream)),
        None => warn!("engine sound: cpal stream nešel vytvořit, ticho"),
    }
}

// Postaví cpal výstupní stream s MALÝM bufferem (nízká latence + jemná kontrola rpm).
// Callback generuje granulární zvuk per-sample a čte sdílené atomiky.
fn build_cpal_stream(shared: GrainShared) -> Option<cpal::Stream> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let default_cfg = device.default_output_config().ok()?;
    let sample_format = default_cfg.sample_format();
    let channels = default_cfg.channels() as usize;
    let sr = default_cfg.sample_rate().0 as f32;

    // f32 je na macOS/CoreAudio běžný; ostatní formáty zatím ticho (fallback by šel doplnit).
    if sample_format != cpal::SampleFormat::F32 {
        warn!("engine sound: neočekávaný audio formát {sample_format:?}");
        return None;
    }

    let mut config: cpal::StreamConfig = default_cfg.into();
    let grain = Arc::new(make_combustion_grain(sr)); // grain autorovaný na sr zařízení

    // Postaví stream s daným bufferem. Vlastní synth+rec (callback je 'static move).
    let build = |cfg: &cpal::StreamConfig| {
        let mut synth = GrainSynth::new(grain.clone(), shared.clone(), sr);
        let rec = shared.rec.clone();
        device.build_output_stream(
            cfg,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    let s = synth.tick();
                    if let Some(rec) = &rec {
                        if let Ok(mut v) = rec.lock() {
                            v.push(f32::from_bits(synth.shared.rpm.load(Relaxed)));
                        }
                    }
                    for ch in frame.iter_mut() {
                        *ch = s;
                    }
                }
            },
            |e| eprintln!("cpal stream error: {e}"),
            None,
        )
    };

    // Malý buffer ~12 ms (proti Bevy/rodio ~100–250 ms); fallback na výchozí když zařízení
    // Fixed nepodporuje.
    config.buffer_size = cpal::BufferSize::Fixed(512);
    let stream = match build(&config) {
        Ok(s) => s,
        Err(_) => {
            config.buffer_size = cpal::BufferSize::Default;
            build(&config).ok()?
        }
    };
    stream.play().ok()?;
    info!("engine sound: cpal stream {sr:.0} Hz, {channels} ch");
    Some(stream)
}

// Každý snímek: rpm + válce + hlasitost → sdílené atomiky. Decoder si je čte a spouští
// grainy. Ticho když nejsme ve hře / auto není spawnuté / motor chcípl.
fn drive_engine_sound(
    engine: Option<Res<EngineHandle>>,
    state: Res<DrivetrainState>,
    loadout: Res<crate::vehicle::Loadout>,
    sb: Res<SoftBody>,
    app_state: Res<State<AppState>>,
    time: Res<Time>,
    mut log_t: Local<f32>,
) {
    let Some(engine) = engine else { return };
    let playing = *app_state.get() == AppState::Game && !sb.entities.is_empty() && !state.stalled;
    let rpm = state.rpm.max(0.0);
    let gain: f32 = if playing { 0.7 } else { 0.0 };

    engine.0.rpm.store(rpm.to_bits(), Relaxed);
    engine.0.cyl.store(loadout.engine_spec.cylinders as u32, Relaxed);
    engine.0.disp.store(loadout.engine_spec.displacement.to_bits(), Relaxed);
    engine.0.config.store(
        match loadout.engine_spec.config {
            crate::vehicle::EngineConfig::Inline => 0,
            crate::vehicle::EngineConfig::V => 1,
            crate::vehicle::EngineConfig::Boxer => 2,
        },
        Relaxed,
    );
    engine.0.gain.store(gain.to_bits(), Relaxed);

    *log_t += time.delta_secs();
    if playing && *log_t >= 1.0 {
        *log_t = 0.0;
        let firing = rpm / 120.0 * loadout.engine_spec.cylinders as f32;
        info!("engine sound: rpm={:.0} firing={:.0}Hz cyl={}", rpm, firing, loadout.engine_spec.cylinders);
    }

    // DEBUG: v ~9 s vypíš zachycený skutečný zvuk hry do WAV (jen s MF_RPM_TRACE).
    if let Some(rec) = &engine.0.rec {
        if time.elapsed_secs() > 9.0 {
            if let Ok(v) = rec.lock() {
                if v.len() > SAMPLE_RATE as usize {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::File::create("/tmp/rpm_seen.csv") {
                        for (k, s) in v.iter().enumerate() {
                            if k % 10 == 0 { let _ = writeln!(f, "{k},{s}"); }
                        }
                    }
                    info!("engine sound: zachyceno {} rpm vzorků do /tmp/rpm_seen.csv", v.len());
                }
            }
        }
    }
}

// Zápis mono i16 WAV (pro debug capture; sdílí s testy přes write_wav v tests).
fn write_wav_pub(path: &str, pcm: &[i16]) {
    use std::io::Write;
    let Ok(mut f) = std::fs::File::create(path) else { return };
    let data_len = (pcm.len() * 2) as u32;
    let _ = f.write_all(b"RIFF");
    let _ = f.write_all(&(36 + data_len).to_le_bytes());
    let _ = f.write_all(b"WAVEfmt ");
    let _ = f.write_all(&16u32.to_le_bytes());
    let _ = f.write_all(&1u16.to_le_bytes());
    let _ = f.write_all(&1u16.to_le_bytes());
    let _ = f.write_all(&SAMPLE_RATE.to_le_bytes());
    let _ = f.write_all(&(SAMPLE_RATE * 2).to_le_bytes());
    let _ = f.write_all(&2u16.to_le_bytes());
    let _ = f.write_all(&16u16.to_le_bytes());
    let _ = f.write_all(b"data");
    let _ = f.write_all(&data_len.to_le_bytes());
    for s in pcm {
        let _ = f.write_all(&s.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vygeneruj SEAMLESS engine loop do assets/engine_loop.wav (přístup B: sample + speed).
    // Ustálený base_rpm, crossfade konce s přirozeným pokračováním → bezešvá smyčka i se šumem.
    // Spusť: cargo test render_loop -- --ignored --nocapture
    // Uživatel může soubor nahradit SKUTEČNOU nahrávkou motoru (steady na BASE_LOOP_RPM).
    #[test]
    #[ignore]
    fn render_loop() {
        let base = BASE_LOOP_RPM;
        let shared = EngineShared::new();
        shared.gain.store(0.6f32.to_bits(), Relaxed);
        shared.cylinders.store(4, Relaxed);
        shared.rpm.store(base.to_bits(), Relaxed);
        let mut d = EngineDecoder::new(shared.clone());
        for _ in 0..SAMPLE_RATE / 2 { d.next(); } // pre-warm: freq se ustálí na base

        let l = 2 * SAMPLE_RATE as usize; // 2 s smyčka
        let x = SAMPLE_RATE as usize / 20; // 50 ms crossfade
        let buf: Vec<f32> = (0..l + x).map(|_| d.next().unwrap()).collect();
        let mut out = buf[..l].to_vec();
        // Crossfade: hlava (0..x) se smíchá s přirozeným pokračováním (buf[l..l+x]), takže
        // out[0]≈buf[l] navazuje na out[l-1]=buf[l-1] → seam bez cvaknutí.
        for i in 0..x {
            let w = i as f32 / x as f32;
            out[i] = buf[i] * w + buf[l + i] * (1.0 - w);
        }
        let pcm: Vec<i16> = out.iter().map(|s| (s.clamp(-1.0, 1.0) * 30000.0) as i16).collect();
        write_wav("assets/engine_loop.wav", &pcm);
        println!("base_rpm={base}, {l} vzorků");
    }

    // DEFINITIVNÍ REPRO granulárního zvuku: přehraj skutečnou rpm stopu přes GrainDecoder.
    // Spusť: cargo test render_grain -- --ignored --nocapture
    #[test]
    #[ignore]
    fn render_grain() {
        let txt = std::fs::read_to_string("/tmp/rpm_trace.csv").expect("nejdřív MF_RPM_TRACE=1 cargo run");
        let rows: Vec<(f32, f32)> = txt.lines().skip(1).filter_map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            if c.len() == 4 { Some((c[0].parse().ok()?, c[1].parse().ok()?)) } else { None }
        }).collect();
        let shared = GrainShared {
            rpm: Arc::new(AtomicU32::new(850.0f32.to_bits())),
            cyl: Arc::new(AtomicU32::new(4)),
            disp: Arc::new(AtomicU32::new(1.4f32.to_bits())),
            config: Arc::new(AtomicU32::new(0)),
            gain: Arc::new(AtomicU32::new(0.7f32.to_bits())),
            rec: None,
        };
        let grain = Arc::new(make_combustion_grain(SAMPLE_RATE as f32));
        let mut d = GrainSynth::new(grain, shared.clone(), SAMPLE_RATE as f32);
        let mut pcm = Vec::new();
        for w in rows.windows(2) {
            shared.rpm.store(w[0].1.to_bits(), Relaxed);
            let n = ((w[1].0 - w[0].0) * SAMPLE_RATE as f32) as usize;
            for _ in 0..n {
                pcm.push((d.tick().clamp(-1.0, 1.0) * 30000.0) as i16);
            }
        }
        write_wav("/tmp/engine_grain.wav", &pcm);
    }

    // Čistý sweep granulárně (bez závislosti na stopě) k rychlému poslechu.
    #[test]
    #[ignore]
    fn render_grain_sweep() {
        let render = |cyl: u32, disp: f32, name: &str| {
            let shared = GrainShared {
                rpm: Arc::new(AtomicU32::new(850.0f32.to_bits())),
                cyl: Arc::new(AtomicU32::new(cyl)),
                disp: Arc::new(AtomicU32::new(disp.to_bits())),
                config: Arc::new(AtomicU32::new(0)),
                gain: Arc::new(AtomicU32::new(0.7f32.to_bits())),
                rec: None,
            };
            let grain = Arc::new(make_combustion_grain(SAMPLE_RATE as f32));
            let mut d = GrainSynth::new(grain, shared.clone(), SAMPLE_RATE as f32);
            let n = SAMPLE_RATE as usize * 6;
            let mut pcm = Vec::with_capacity(n);
            for i in 0..n {
                let t = i as f32 / n as f32;
                let rpm = if t < 0.5 { 850.0 + 6150.0 * (t * 2.0) } else { 7000.0 - 6150.0 * ((t - 0.5) * 2.0) };
                shared.rpm.store(rpm.to_bits(), Relaxed);
                pcm.push((d.tick().clamp(-1.0, 1.0) * 30000.0) as i16);
            }
            write_wav(&format!("/tmp/engine_grain_{name}.wav"), &pcm);
        };
        render(3, 1.0, "i3_1l");   // malý tříválec — vysoký, tinny
        render(4, 1.4, "i4_1p4l"); // běžný i4
        render(8, 5.0, "v8_5l");   // velkoobjemový V8 — hluboký, hustý
        render(12, 6.0, "v12_6l"); // V12 — velmi hustý
    }

    // Porovnání uspořádání válců (burble) — steady nízké otáčky, kde je pattern slyšet.
    #[test]
    #[ignore]
    fn render_config() {
        let render = |cfg: u32, name: &str| {
            let shared = GrainShared {
                rpm: Arc::new(AtomicU32::new(1300.0f32.to_bits())),
                cyl: Arc::new(AtomicU32::new(8)),
                disp: Arc::new(AtomicU32::new(5.0f32.to_bits())),
                config: Arc::new(AtomicU32::new(cfg)),
                gain: Arc::new(AtomicU32::new(0.7f32.to_bits())),
                rec: None,
            };
            let grain = Arc::new(make_combustion_grain(SAMPLE_RATE as f32));
            let mut d = GrainSynth::new(grain, shared.clone(), SAMPLE_RATE as f32);
            let pcm: Vec<i16> = (0..SAMPLE_RATE as usize * 4).map(|_| (d.tick().clamp(-1.0, 1.0) * 30000.0) as i16).collect();
            write_wav(&format!("/tmp/engine_cfg_{name}.wav"), &pcm);
        };
        render(0, "inline"); // rovnoměrně
        render(1, "v");      // burble
        render(2, "boxer");  // silnější offbeat rumble
    }

    #[test]
    fn firing_params_sane() {
        // Decay kladný, formant ve slyšitelném/motorovém pásmu pro všechny configy/válce.
        for cfg in 0..3 {
            for cyl in [3.0, 4.0, 6.0, 8.0] {
                let (decay, bright, noise_amt) = firing_params(cyl, cfg);
                assert!(decay > 0.0 && decay.is_finite(), "decay={decay}");
                assert!((0.0..=2.0).contains(&bright), "bright={bright}");
                assert!((0.0..=1.0).contains(&noise_amt), "noise_amt={noise_amt}");
            }
        }
    }

    // Pomocník: zapiš mono i16 PCM jako WAV.
    fn write_wav(path: &str, pcm: &[i16]) {
        use std::io::Write;
        let mut f = std::fs::File::create(path).unwrap();
        let data_len = (pcm.len() * 2) as u32;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
        f.write_all(&(SAMPLE_RATE * 2).to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        for s in pcm {
            f.write_all(&s.to_le_bytes()).unwrap();
        }
        println!("wrote {path}");
    }

    // DEFINITIVNÍ REPRO: přehraj SKUTEČNOU nahranou rpm stopu (/tmp/rpm_trace.csv) přes
    // decoder — přesně to, co zní ve hře. rpm se mění v časech ze stopy (60Hz jitter).
    // Spusť: cargo test render_trace -- --ignored --nocapture
    #[test]
    #[ignore]
    fn render_trace() {
        let txt = std::fs::read_to_string("/tmp/rpm_trace.csv").expect("nejdřív spusť MF_RPM_TRACE=1 cargo run");
        let rows: Vec<(f32, f32)> = txt.lines().skip(1).filter_map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            if c.len() == 4 { Some((c[0].parse().ok()?, c[1].parse().ok()?)) } else { None }
        }).collect();
        let shared = EngineShared::new();
        shared.gain.store(0.5f32.to_bits(), Relaxed);
        shared.cylinders.store(4, Relaxed);
        let mut d = EngineDecoder::new(shared.clone());
        let mut pcm = Vec::new();
        for w in rows.windows(2) {
            let (t0, rpm) = w[0];
            let (t1, _) = w[1];
            shared.rpm.store(rpm.to_bits(), Relaxed);
            let n = ((t1 - t0) * SAMPLE_RATE as f32) as usize;
            for _ in 0..n {
                pcm.push((d.next().unwrap() * 30000.0) as i16);
            }
        }
        write_wav("/tmp/engine_trace.wav", &pcm);
    }

    // Dočasné: vyrenderuj ukázkový WAV (sweep otáček) k poslechu bez spouštění hry.
    // Spusť: cargo test render_wav -- --ignored --nocapture
    #[test]
    #[ignore]
    fn render_wav() {
        use std::io::Write;
        let render = |cyl: u32, cfg: u32, name: &str| {
            let shared = EngineShared::new();
            shared.gain.store(0.5f32.to_bits(), Relaxed);
            shared.cylinders.store(cyl, Relaxed);
            shared.config.store(cfg, Relaxed);
            let mut d = EngineDecoder::new(shared.clone());
            let secs = 6.0;
            let n = (SAMPLE_RATE as f32 * secs) as usize;
            let mut pcm: Vec<i16> = Vec::with_capacity(n);
            let frame = SAMPLE_RATE as usize / 60; // rpm se aktualizuje 60×/s jako ve hře
            // Profil otáček: pomalý sweep nahoru/dolů + dva prudké „floor-it" bliky na konci
            // (idle→redline za 0.3 s) = stres test responzivnosti při frame-rate updatu.
            let rpm_at = |t: f32| -> f32 {
                if t < 0.4 {
                    850.0 + 6150.0 * (t / 0.4) // sweep nahoru
                } else if t < 0.6 {
                    7000.0 - 6150.0 * ((t - 0.4) / 0.2) // sweep dolů
                } else {
                    // blipy: rychlé nakopnutí a puštění
                    let p = ((t - 0.6) / 0.1) % 2.0;
                    if p < 1.0 { 850.0 + 6150.0 * p } else { 850.0 + 6150.0 * (2.0 - p) }
                }
            };
            for i in 0..n {
                if i % frame == 0 {
                    shared.rpm.store(rpm_at(i as f32 / n as f32).to_bits(), Relaxed);
                }
                let s = d.next().unwrap();
                pcm.push((s * 30000.0) as i16);
            }
            let path = format!("/tmp/engine_{name}.wav");
            let mut f = std::fs::File::create(&path).unwrap();
            let data_len = (pcm.len() * 2) as u32;
            f.write_all(b"RIFF").unwrap();
            f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
            f.write_all(b"WAVEfmt ").unwrap();
            f.write_all(&16u32.to_le_bytes()).unwrap();
            f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
            f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
            f.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
            f.write_all(&(SAMPLE_RATE * 2).to_le_bytes()).unwrap(); // byte rate
            f.write_all(&2u16.to_le_bytes()).unwrap(); // block align
            f.write_all(&16u16.to_le_bytes()).unwrap(); // bits
            f.write_all(b"data").unwrap();
            f.write_all(&data_len.to_le_bytes()).unwrap();
            for s in &pcm {
                f.write_all(&s.to_le_bytes()).unwrap();
            }
            println!("wrote {path}");
        };
        render(4, 0, "i4");
        render(8, 1, "v8");
        render(4, 2, "boxer4");
    }

    #[test]
    fn decoder_output_bounded() {
        // Finální výstup decoderu (pulz+šum+filtry) nikdy nepřeteče ±1.
        let shared = EngineShared::new();
        shared.rpm.store(4500.0f32.to_bits(), Relaxed);
        shared.gain.store(0.5f32.to_bits(), Relaxed);
        let mut d = EngineDecoder::new(shared);
        for _ in 0..SAMPLE_RATE {
            let s = d.next().unwrap();
            assert!(s.is_finite() && s.abs() <= 1.0, "out of range: {s}");
        }
    }

    #[test]
    fn decoder_tracks_rpm_freq() {
        // Po zapsání rpm decoder produkuje konečné vzorky a fáze postupuje.
        let shared = EngineShared::new();
        shared.rpm.store(3000.0f32.to_bits(), Relaxed);
        shared.gain.store(0.1f32.to_bits(), Relaxed);
        let mut d = EngineDecoder::new(shared);
        for _ in 0..SAMPLE_RATE {
            let s = d.next().unwrap();
            assert!(s.is_finite());
        }
        // 3000 rpm, 4 válce → 100 Hz zápalů; freq se vyhladila k cíli.
        assert!((d.freq - 100.0).abs() < 5.0, "freq={}", d.freq);
    }
}
