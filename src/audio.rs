// audio — herní zvuky přes vlastní cpal výstup (nízká latence, obchází Bevy audio ~250ms
// buffer). Motor = GRANULÁRNÍ SYNTÉZA: jeden "combustion" grain se spouští na zápalové
// frekvenci (rpm×válce) s překrýváním voices; pitch = časování spouštění (fázový akumulátor
// v audio-čase). Herní systém píše stav do sdílených atomiků, cpal callback je čte a mixuje
// motor + skřípot pneu + blip řazení + bounce omezovače + crash + vítr.

use bevy::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

use crate::solver::SoftBody;
use crate::ui::AppState;
use crate::vehicle::DrivetrainState;

const SAMPLE_RATE: u32 = 44_100;

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
    master: Arc<AtomicU32>,   // celková hlasitost 0..1 (škáluje engine+skřípot+blip, f32 v bitech)
    screech_gain: Arc<AtomicU32>, // samostatná hlasitost skřípotu pneu 0..1 (f32 v bitech)
    crash_gain: Arc<AtomicU32>,   // samostatná hlasitost nárazů 0..1 (f32 v bitech)
    skid: Arc<AtomicU32>,     // skřípot pneu: 0=grip, 1=plný prokluz (f32 v bitech)
    max_rpm: Arc<AtomicU32>,  // omezovač (redline) pro bounce (f32 v bitech)
    shift_seq: Arc<AtomicU32>, // počítadlo řazení — změna = spusť blip
    shift_dir: Arc<AtomicU32>, // 0=podřazení (rev blip), 1=nahoru
    crash_seq: Arc<AtomicU32>, // počítadlo nárazů — změna = spusť crash
    crash_mag: Arc<AtomicU32>, // síla nárazu 0..1 (f32 v bitech)
    speed: Arc<AtomicU32>,     // rychlost km/h pro vítr (f32 v bitech)
    wind_gain: Arc<AtomicU32>, // samostatná hlasitost větru 0..1 (f32 v bitech)
    brake: Arc<AtomicU32>,     // intenzita pískání brzd 0..1 (f32 v bitech)
    surface: Arc<AtomicU32>,   // rachot povrchu (štěrk/tráva) 0..1 (f32 v bitech)
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
    // Skřípot pneu: vyhlazená intenzita + warble LFO + fáze tónového oscilátoru (squeal).
    skid: f32,
    warble: f32,
    sq_phase: f32,
    // Blip řazení: obálka + směr + naposledy viděné počítadlo + fáze tónu.
    shift_env: f32,
    shift_dir: u32,
    shift_seq_seen: u32,
    blip_phase: f32,
    // Bounce omezovače: fáze stutter gate.
    limiter_phase: f32,
    // Crash: obálka + naposledy viděné počítadlo + síla + fáze boomu + lowpass crunche.
    crash_env: f32,
    crash_seq_seen: u32,
    crash_mag: f32,
    crash_phase: f32,
    crash_lp: f32,
    // Vítr: vyhlazená rychlost + bandpass šumu (whoosh vzduchu).
    wind_spd: f32,
    wind_lp: f32,
    wind_lp2: f32,
    // Pískání brzd: vyhlazená intenzita + dvě fáze tónu (beating) + náhodný gate (intermitence).
    brake_amt: f32,
    brake_phase: f32,
    brake_phase2: f32,
    brake_gate: f32,     // vyhlazený stav gate (0=ticho, 1=píská)
    brake_gate_tgt: f32, // cíl gate (náhodně 0/1)
    brake_gtimer: f32,   // samply do dalšího losování gate
    // Rachot povrchu: vyhlazená intenzita + rumble lowpass + jasnější crackle band (kamínky).
    surf_amt: f32,
    surf_lp: f32,
    surf_hp: f32,
}

impl GrainSynth {
    fn new(grain: Arc<Vec<f32>>, shared: GrainShared, sr: f32) -> Self {
        Self { grain, shared, sr, phase: 0.0, freq: 0.0, gain: 0.0, norm: 1.0, lp: 0.0, next_thresh: 1.0, fire_i: 0, voices: Vec::with_capacity(64), rng: 0x2545f491, skid: 0.0, warble: 0.0, sq_phase: 0.0, shift_env: 0.0, shift_dir: 1, shift_seq_seen: 0, blip_phase: 0.0, limiter_phase: 0.0, crash_env: 0.0, crash_seq_seen: 0, crash_mag: 0.0, crash_phase: 0.0, crash_lp: 0.0, wind_spd: 0.0, wind_lp: 0.0, wind_lp2: 0.0, brake_amt: 0.0, brake_phase: 0.0, brake_phase2: 0.0, brake_gate: 0.0, brake_gate_tgt: 0.0, brake_gtimer: 0.0, surf_amt: 0.0, surf_lp: 0.0, surf_hp: 0.0 }
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
        let mut engine = (self.lp * 1.2).tanh();

        // BOUNCE OMEZOVAČE: u redline řízne palivo → stutter „bap-bap". Rychlý gate dusí engine
        // hlasitost, čím blíž limitu tím hlouběji. rpm už fyzika drží u max (moment 0), tohle
        // přidá slyšitelné sekání. ponytail: 28 Hz a hloubka 0.85 jsou knoby.
        let max_rpm = f32::from_bits(self.shared.max_rpm.load(Relaxed));
        if max_rpm > 0.0 {
            let over = ((rpm - (max_rpm - 250.0)) / 250.0).clamp(0.0, 1.0);
            if over > 0.0 {
                self.limiter_phase += 28.0 / self.sr;
                if self.limiter_phase >= 1.0 { self.limiter_phase -= 1.0; }
                let gate = if self.limiter_phase < 0.5 { 1.0 } else { 1.0 - 0.85 * over };
                engine *= gate;
            }
        }

        // BLIP ŘAZENÍ: změna počítadla = spusť krátkou obálku. Podřazení = vyšší „rev" blip,
        // nahoru = nižší/tupější (throttle cut). Aditivní tón + šum, ~120 ms exp decay.
        // ponytail: freq, decay 0.9993 a 0.3 gain jsou knoby.
        let seq = self.shared.shift_seq.load(Relaxed);
        if seq != self.shift_seq_seen {
            self.shift_seq_seen = seq;
            self.shift_env = 1.0;
            self.shift_dir = self.shared.shift_dir.load(Relaxed);
        }
        let blip = if self.shift_env > 0.001 {
            use std::f32::consts::TAU;
            self.shift_env *= 0.9993;
            let bf = if self.shift_dir == 0 { 230.0 } else { 150.0 };
            self.blip_phase += bf / self.sr;
            if self.blip_phase >= 1.0 { self.blip_phase -= 1.0; }
            let ph = self.blip_phase * TAU;
            let tone = ph.sin() * 0.5 + (2.0 * ph).sin() * 0.3;
            (tone + self.noise() * 0.4) * self.shift_env * 0.3
        } else {
            0.0
        };

        // CRASH: náraz do překážky. Hluboký „boom" (nízké harmonické) + tlumený šumový „crunch"
        // (deformace/rachot). Delší decay než blip (~400 ms). Hlasitost dle síly nárazu.
        // ponytail: freq, decay 0.9997 a poměr boom/crunch jsou knoby.
        let crash_seq = self.shared.crash_seq.load(Relaxed);
        if crash_seq != self.crash_seq_seen {
            self.crash_seq_seen = crash_seq;
            self.crash_env = 1.0;
            self.crash_mag = f32::from_bits(self.shared.crash_mag.load(Relaxed)).clamp(0.0, 1.0);
        }
        let crash = if self.crash_env > 0.001 {
            use std::f32::consts::TAU;
            self.crash_env *= 0.9997;
            self.crash_phase += 65.0 / self.sr; // hluboký boom ~65 Hz
            if self.crash_phase >= 1.0 { self.crash_phase -= 1.0; }
            let ph = self.crash_phase * TAU;
            let boom = ph.sin() * 0.6 + (2.0 * ph).sin() * 0.3 + (3.0 * ph).sin() * 0.15;
            // Crunch: lowpassnutý šum (kovový rachot, ne sykot).
            self.crash_lp += (self.noise() - self.crash_lp) * 0.25;
            let crunch = self.crash_lp * 0.7;
            let cg = f32::from_bits(self.shared.crash_gain.load(Relaxed));
            (boom + crunch) * self.crash_env * self.crash_mag * 0.9 * cg
        } else {
            0.0
        };

        // VÍTR: bandpassnutý šum (whoosh vzduchu), hlasitost i jas jemně rostou s rychlostí.
        // HODNĚ subtle — jen náznak na vysokých rychlostech, nesmí přebít motor. Náběh od ~30
        // km/h, plno ~180. ponytail: práh/max/0.045 gain jsou knoby.
        let spd_t = f32::from_bits(self.shared.speed.load(Relaxed));
        self.wind_spd += (spd_t - self.wind_spd) * 0.02;
        let wind = {
            let w = ((self.wind_spd - 30.0) / 150.0).clamp(0.0, 1.0);
            if w > 0.001 {
                let n = self.noise();
                let a_hi = 0.10 + 0.20 * w; // jasnější s rychlostí
                self.wind_lp += (n - self.wind_lp) * a_hi;
                self.wind_lp2 += (self.wind_lp - self.wind_lp2) * 0.02; // spodní řez → bandpass
                let wg = f32::from_bits(self.shared.wind_gain.load(Relaxed));
                (self.wind_lp - self.wind_lp2) * w * 0.045 * wg
            } else {
                0.0
            }
        };

        // PÍSKÁNÍ BRZD: brzdový squeal je self-excited limit-cycle = near-pure TÓN (ne šum!),
        // fundament + harmonické vázané na vlastní frekvence kotouče. Beru high-freq squeal
        // (nejčastější): fundament ~3 kHz + 2./3. harmonická (kovová barva). DVĚ mírně rozladěné
        // složky → beating (víc blízkých modů = charakteristické chvění „wíííe"). Špetka šumu =
        // rasp. Tonální jak skřípot pneu, ne šumový filtr (ten zněl jak „šššš").
        // ponytail: fc 3 kHz, beating 1.003 a 0.11 gain jsou knoby.
        let brake_t = f32::from_bits(self.shared.brake.load(Relaxed));
        self.brake_amt += (brake_t - self.brake_amt) * 0.03;
        let brake_sq = if self.brake_amt > 0.002 {
            use std::f32::consts::TAU;
            // INTERMITENCE: squeal NENÍ konstantní — mode-coupling nestabilita překročí práh jen
            // občas, tak se náhodně spíná do krátkých návalů (většinu času ticho i při brzdění).
            if self.brake_gtimer <= 0.0 {
                self.brake_gate_tgt = if self.noise() * 0.5 + 0.5 < 0.28 { 1.0 } else { 0.0 }; // ~28 % času
                self.brake_gtimer = self.sr * (0.2 + 0.5 * (self.noise() * 0.5 + 0.5)); // naval 0.2..0.7 s
            }
            self.brake_gtimer -= 1.0;
            self.brake_gate += (self.brake_gate_tgt - self.brake_gate) * 0.015; // plynulý náběh/dozvuk
            // Fundament ~1.3 kHz (níž = míň piercing) + 2. harmonická; dvě rozladěné složky = beating.
            let fc = 1300.0;
            self.brake_phase += fc / self.sr;
            if self.brake_phase >= 1.0 { self.brake_phase -= 1.0; }
            self.brake_phase2 += fc * 1.004 / self.sr; // rozladěná složka → beating
            if self.brake_phase2 >= 1.0 { self.brake_phase2 -= 1.0; }
            let (p, p2) = (self.brake_phase * TAU, self.brake_phase2 * TAU);
            let tone = (p.sin() + p2.sin()) * 0.45 + (2.0 * p).sin() * 0.18;
            (tone + self.noise() * 0.04) * self.brake_amt * self.brake_gate * 0.1
        } else {
            0.0
        };

        // RACHOT POVRCHU: dvě vrstvy — nízké dunění (rumble, lowpass) + jasnější „crackle" band
        // (kamínky odskakující od podvozku). Jen rumble zněl jak vítr; crackle dá štěrku texturu.
        // Intenzita = rychlost × drsnost mapy; asfalt (TestPad) = 0 = ticho. ponytail: 0.12 knob.
        let surf_t = f32::from_bits(self.shared.surface.load(Relaxed));
        self.surf_amt += (surf_t - self.surf_amt) * 0.03;
        let surface = if self.surf_amt > 0.002 {
            let n = self.noise();
            self.surf_lp += (n - self.surf_lp) * 0.08; // rumble (nízké dunění)
            self.surf_hp += (n - self.surf_hp) * 0.5;  // rychlejší lp → střední pásmo
            let crackle = self.surf_hp - self.surf_lp; // rozdíl pásem = mid „crackle"
            (self.surf_lp * 0.75 + crackle * 0.55) * self.surf_amt * 0.12
        } else {
            0.0
        };

        // SKŘÍPOT PNEU: TÓNOVÝ oscilátor (ne šum) = reálný squeal ze stick-slipu. Šumem buzený
        // filtr zněl jak koberec/vítr; pneu píská čistý tón s harmonickými. Základ + 2./3.
        // harmonická = reedy/kovová barva. Warble LFO vlní výšku (charakteristické kvílení).
        // Špetka šumu = grit, ať to není holá pískačka. Jde MIMO engine lowpass (má být jasné).
        let skid_t = f32::from_bits(self.shared.skid.load(Relaxed));
        self.skid += (skid_t - self.skid) * 0.02;
        let screech = if self.skid > 0.002 {
            use std::f32::consts::TAU;
            self.warble += 6.0 / self.sr; // ~6 Hz vlnění výšky
            if self.warble >= 1.0 { self.warble -= 1.0; }
            let warble = 1.0 + 0.06 * (TAU * self.warble).sin();
            // Základní výška 480..820 Hz dle skluzu (hlubší squeal). Pitch dle SKLUZU
            // (clamp 1.0) — teplotní zesílení jde jen do hlasitosti (× self.skid níž), ne výšky.
            let fc = (480.0 + 340.0 * self.skid.min(1.0)) * warble;
            self.sq_phase += fc / self.sr;
            if self.sq_phase >= 1.0 { self.sq_phase -= self.sq_phase.floor(); }
            let ph = self.sq_phase * TAU;
            // Základ + harmonické (klesající) = reedy squeal, ne čistá sinus-pískačka.
            let tone = ph.sin() * 0.6 + (2.0 * ph).sin() * 0.3 + (3.0 * ph).sin() * 0.15;
            let grit = self.noise() * 0.08; // špetka šumu = tření, ne holý tón
            // Hlasitost lineárně s intenzitou skluzu (dřív skid^1.5 dusilo nízký skluz) × samostatná
            // hlasitost skřípotu ze settings. ponytail: fc rozsah, harmonické, warble, grit, 0.6 knoby.
            let sg = f32::from_bits(self.shared.screech_gain.load(Relaxed));
            (tone + grit) * self.skid * 0.6 * sg
        } else {
            0.0
        };

        let master = f32::from_bits(self.shared.master.load(Relaxed));
        ((engine + screech + blip + crash + wind + brake_sq + surface) * master).clamp(-1.0, 1.0)
    }
}

#[derive(Resource, Clone)]
struct EngineHandle(GrainShared);

// Nastavení hlasitosti (0..1), měněné z pause menu. Audio čte přes drive_engine_sound.
#[derive(Resource)]
pub struct AudioSettings {
    pub master: f32,
    pub engine: f32,
    pub screech: f32,
    pub crash: f32,
    pub wind: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { master: 0.8, engine: 0.8, screech: 0.8, crash: 0.8, wind: 0.8 }
    }
}

// Drží živý cpal stream (jinak by se dropnul a ztichl). !Send → NonSend resource.
struct AudioStream(#[allow(dead_code)] cpal::Stream);

pub struct EngineSoundPlugin;

impl Plugin for EngineSoundPlugin {
    fn build(&self, app: &mut App) {
        // Exkluzivní startup systém: cpal::Stream je !Send → musí přes NonSend resource.
        app.init_resource::<AudioSettings>()
            .add_systems(Startup, setup_engine_audio)
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
        master: Arc::new(AtomicU32::new(0.8f32.to_bits())),
        screech_gain: Arc::new(AtomicU32::new(0.8f32.to_bits())),
        crash_gain: Arc::new(AtomicU32::new(0.8f32.to_bits())),
        skid: Arc::new(AtomicU32::new(0.0f32.to_bits())),
        max_rpm: Arc::new(AtomicU32::new(0.0f32.to_bits())),
        shift_seq: Arc::new(AtomicU32::new(0)),
        shift_dir: Arc::new(AtomicU32::new(1)),
        crash_seq: Arc::new(AtomicU32::new(0)),
        crash_mag: Arc::new(AtomicU32::new(0.0f32.to_bits())),
        speed: Arc::new(AtomicU32::new(0.0f32.to_bits())),
        wind_gain: Arc::new(AtomicU32::new(0.8f32.to_bits())),
        brake: Arc::new(AtomicU32::new(0.0f32.to_bits())),
        surface: Arc::new(AtomicU32::new(0.0f32.to_bits())),
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
    settings: Res<AudioSettings>,
    map: Res<crate::ui::SelectedMap>,
    time: Res<Time>,
    mut crash_ev: EventReader<crate::solver::CrashImpact>,
    mut log_t: Local<f32>,
    mut prev_gear: Local<Option<i32>>,
    mut crash_cd: Local<f32>,
) {
    let Some(engine) = engine else { return };
    let playing = *app_state.get() == AppState::Game && !sb.entities.is_empty() && !state.stalled;
    let rpm = state.rpm.max(0.0);
    let gain: f32 = if playing { 0.7 * settings.engine } else { 0.0 };
    engine.0.master.store(settings.master.to_bits(), Relaxed);
    engine.0.screech_gain.store(settings.screech.to_bits(), Relaxed);
    engine.0.crash_gain.store(settings.crash.to_bits(), Relaxed);
    engine.0.speed.store(if playing { state.speed_kmh.abs() } else { 0.0 }.to_bits(), Relaxed);
    engine.0.wind_gain.store(settings.wind.to_bits(), Relaxed);

    let spd = state.speed_kmh.abs();

    // SKŘÍPOT PNEU (skid): nejvyšší prokluz přes kola — podélný (κ) i boční (α) vůči onset
    // prahům. THERMAL: škáluje se teplotou POVRCHU pneu — studená/přehřátá guma protestuje
    // víc (tire_squeal_gain). Per kolo, bereme nejhlasitější.
    let skid = if playing {
        const K0: f32 = 0.22; // podélný skluz onsetu
        const A0: f32 = 0.20; // boční skluz onsetu (rad)
        sb.wheels.iter().filter(|w| w.telem.fz > 0.0).fold(0.0f32, |acc, w| {
            let t = &w.telem;
            let comb = ((t.slip_ratio / K0).powi(2) + (t.slip_angle / A0).powi(2)).sqrt();
            let base = ((comb - 1.0) / 2.0).clamp(0.0, 1.0);
            acc.max(base * crate::thermal::tire_squeal_gain(w.thermal.tire_surf_c))
        })
    } else {
        0.0
    };
    engine.0.skid.store(skid.to_bits(), Relaxed);

    // PÍSKÁNÍ BRZD: jen při PROKLUZU destiček↔disku — brzda přitlačí (brake>1, na zemi) a disk
    // se TOČÍ (spin_vel). Zamčené kolo (spin_vel→0) = žádný prokluz = ticho (převezme screech
    // pneu). THERMAL: studené destičky pískají víc (brake_c: okolí=plný squeal, ~200°C=ticho).
    let brake = if playing {
        sb.wheels.iter()
            .filter(|w| w.brake > 1.0 && w.telem.fz > 0.0)
            .fold(0.0f32, |acc, w| {
                let disc_slip = (w.spin_vel.abs() / 5.0).clamp(0.0, 1.0); // prokluz destiček↔disku
                let cold = ((200.0 - w.thermal.brake_c) / 170.0).clamp(0.0, 1.0);
                acc.max(cold * disc_slip)
            })
    } else {
        0.0
    };
    engine.0.brake.store(brake.to_bits(), Relaxed);

    // RACHOT POVRCHU: drsnost mapy × rychlost, jen kola na zemi. Asfalt (TestPad) = ticho.
    let rough = match map.0 {
        crate::world::MapKind::TestPad => 0.0,
        crate::world::MapKind::Hills => 0.7,
        crate::world::MapKind::Real => 0.85,
    };
    let surface = if playing && rough > 0.0 && sb.wheels.iter().any(|w| w.telem.fz > 0.0) {
        rough * (spd / 40.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    engine.0.surface.store(surface.to_bits(), Relaxed);

    // BLIP ŘAZENÍ: změna kvaltu za jízdy → bump počítadla + směr (audio vlákno spustí obálku).
    // To je vlastně BOV/turbo flutter — zní jen s turbem (boost>0) a při vysokých otáčkách
    // (nad ~60 % redline), kde je v sání tlak. Bez turba / při nízkých otáčkách řazení tiché.
    // prev_gear None = mimo hru / první snímek → jen zapamatuj, žádný blip (ať re-entry necvakne).
    if playing {
        if let Some(pg) = *prev_gear {
            let boosted = loadout.engine_spec.boost > 0.0 && rpm > 0.6 * loadout.engine_spec.redline;
            if state.gear != pg && boosted {
                engine.0.shift_dir.store(if state.gear > pg { 1 } else { 0 }, Relaxed);
                engine.0.shift_seq.fetch_add(1, Relaxed);
            }
        }
        *prev_gear = Some(state.gear);
    } else {
        *prev_gear = None;
    }
    // Omezovač pro bounce (0 = neplatné → audio vypne, když nehrajeme).
    engine.0.max_rpm.store(if playing { loadout.engine_spec.redline } else { 0.0 }.to_bits(), Relaxed);

    // CRASH: nejsilnější náraz z eventů → jeden crash s cooldownem (ať se rány neslejou).
    *crash_cd = (*crash_cd - time.delta_secs()).max(0.0);
    let peak = crash_ev.read().fold(0.0f32, |a, e| a.max(e.0));
    if peak > 0.0 && *crash_cd <= 0.0 {
        let mag = (peak / 12.0).clamp(0.25, 1.0); // ~12 m/s = plný crash
        engine.0.crash_mag.store(mag.to_bits(), Relaxed);
        engine.0.crash_seq.fetch_add(1, Relaxed);
        *crash_cd = 0.25;
    }

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
        info!("engine sound: rpm={:.0} firing={:.0}Hz cyl={} skid={:.2}", rpm, firing, loadout.engine_spec.cylinders, skid);
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

#[cfg(test)]
mod tests {
    use super::*;


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
            master: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            screech_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            crash_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            skid: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            max_rpm: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            shift_seq: Arc::new(AtomicU32::new(0)),
            shift_dir: Arc::new(AtomicU32::new(1)),
            crash_seq: Arc::new(AtomicU32::new(0)),
            crash_mag: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            speed: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            wind_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            brake: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            surface: Arc::new(AtomicU32::new(0.0f32.to_bits())),
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
                master: Arc::new(AtomicU32::new(1.0f32.to_bits())),
                screech_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
                crash_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
                skid: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                max_rpm: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                shift_seq: Arc::new(AtomicU32::new(0)),
                shift_dir: Arc::new(AtomicU32::new(1)),
                crash_seq: Arc::new(AtomicU32::new(0)),
                crash_mag: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                speed: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                wind_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
                brake: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                surface: Arc::new(AtomicU32::new(0.0f32.to_bits())),
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
                master: Arc::new(AtomicU32::new(1.0f32.to_bits())),
                screech_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
                crash_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
                skid: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                max_rpm: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                shift_seq: Arc::new(AtomicU32::new(0)),
                shift_dir: Arc::new(AtomicU32::new(1)),
                crash_seq: Arc::new(AtomicU32::new(0)),
                crash_mag: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                speed: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                wind_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
                brake: Arc::new(AtomicU32::new(0.0f32.to_bits())),
                surface: Arc::new(AtomicU32::new(0.0f32.to_bits())),
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

    // Vyrenderuj jízdní zvuky (brake/surface/wind) samostatně do /tmp k poslechu (engine ztlumen).
    // Spusť: cargo test render_extras -- --ignored --nocapture
    #[test]
    #[ignore]
    fn render_extras() {
        let shared = GrainShared {
            rpm: Arc::new(AtomicU32::new(850.0f32.to_bits())),
            cyl: Arc::new(AtomicU32::new(4)),
            disp: Arc::new(AtomicU32::new(1.4f32.to_bits())),
            config: Arc::new(AtomicU32::new(0)),
            gain: Arc::new(AtomicU32::new(0.0f32.to_bits())), // engine ticho — jen jízdní zvuk
            master: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            screech_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            crash_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            skid: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            max_rpm: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            shift_seq: Arc::new(AtomicU32::new(0)),
            shift_dir: Arc::new(AtomicU32::new(1)),
            crash_seq: Arc::new(AtomicU32::new(0)),
            crash_mag: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            speed: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            wind_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            brake: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            surface: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            rec: None,
        };
        let grain = Arc::new(make_combustion_grain(SAMPLE_RATE as f32));
        // Fresh synth per zvuk (reset filtrů/obálek); 3 s render.
        let mut render = |name: &str| {
            let mut d = GrainSynth::new(grain.clone(), shared.clone(), SAMPLE_RATE as f32);
            let pcm: Vec<i16> = (0..SAMPLE_RATE as usize * 3).map(|_| (d.tick().clamp(-1.0, 1.0) * 30000.0) as i16).collect();
            write_wav(&format!("/tmp/sound_{name}.wav"), &pcm);
        };
        shared.brake.store(1.0f32.to_bits(), Relaxed);
        render("brake");
        shared.brake.store(0.0f32.to_bits(), Relaxed);
        shared.surface.store(1.0f32.to_bits(), Relaxed);
        render("surface");
        shared.surface.store(0.0f32.to_bits(), Relaxed);
        shared.speed.store(180.0f32.to_bits(), Relaxed);
        render("wind");
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

}
