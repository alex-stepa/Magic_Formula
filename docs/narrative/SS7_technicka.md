# SS7 — technická stránka

Doprovodný dokument k [`SS7_pribeh.md`](SS7_pribeh.md). Tady je návrh, **jak**
to postavit, ne finální kód — je to plán k rozpracování, ne hotová
implementace.

## 0. Tvrdý požadavek: izolace

SS7 je **samostatný gamemode**, ne feature uvnitř rally hry. Konkrétně:

- **Žádná** závislost na `SoftBody`, `solver.rs`, `vehicle.rs` (drivetrain,
  suspension, tire model), `veh.rs`.
- Smí sdílet: `world.rs` (`Terrain`/`World` trait) — pokud se v módu bude
  jezdit mezi lokacemi, ale zjednodušeně, ne přes node-beam solver.
- Smí sdílet render/asset infrastrukturu (meshe, materiály, textury) a obecné
  Bevy pluginy z `main.rs`.
- Pokud se něco z tohohle módu ukáže jako obecně užitečné pro hlavní hru
  (např. dialog systém), přesune se to zpětně jako sdílený modul — ale
  směr návrhu je vždy "archive mode používá věci z core", ne naopak.

Důvod: fáze 2 (Physics) je aktivní vývoj s výkonově citlivým solverem.
Narativní mode nesmí přidat žádnou zátěž ani riziko regrese do toho, co
právě funguje.

## 1. Vstupní bod — `AppState`

Aktuálně (`main.rs`) hra nemá žádný stavový automat — je to jedna plochá
scéna, menu se přepíná přes resource/flag (`toggle_menu`). Pro dva
gamemody vedle sebe potřebuješ Bevy `States`:

```rust
#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
enum AppState {
    #[default]
    MainMenu,   // výběr: Rally / Archive
    Rally,      // dnešní obsah main.rs beze změny
    Archive,    // nový narativní mode
}
```

- `App::init_state::<AppState>()` v `main.rs`.
- Rally systémy (`softbody_solve`, `vehicle_drive`, …) se gatují
  `.run_if(in_state(AppState::Rally))` — beze změny logiky, jen podmínka
  navíc.
- Archive systémy běží jen v `AppState::Archive`.
- Přechod mezi stavy = `OnEnter`/`OnExit` pro spawn/despawn scény (Bevy
  `StateScoped` komponenta se hodí — entity se samy uklidí při výstupu ze
  stavu).

Tohle je jediná změna, co se dotkne `main.rs` napřímo. Zbytek žije v
novém modulu.

## 2. Struktura modulu

```
src/
├── archive/
│   ├── mod.rs          ← ArchivePlugin, registruje sub-systémy a resources
│   ├── investigation.rs← Investigation resource (stav vyšetřování, flagy)
│   ├── documents.rs     ← Document data + document viewer UI
│   ├── dialogue.rs       ← Dialogue systém (větvení, podmínky, pauzy)
│   ├── locations.rs      ← lokace + jednoduchý cestovní systém
│   └── data/              ← RON/JSON soubory s obsahem (dokumenty, dialogy)
```

`mod archive;` přidat do `main.rs` vedle ostatních `mod` deklarací.

## 3. Datový model

### `Investigation` — resource

Centrální stav toho, co hráč ví. Řídí, které dokumenty/dialogové větve
jsou dostupné, a co se ukáže v závěru (akt 3, bod 10 v `SS7_pribeh.md`).

```rust
#[derive(Resource, Default)]
struct Investigation {
    found_documents: HashSet<DocumentId>,
    dialogue_flags: HashSet<Flag>,      // např. Flag::HoloubekAdmittedContact
    holoubek_pressure: u8,               // kolikrát hráč tlačil v rozhovoru
}
```

Jednoduchý set/counter, žádný komplexní graf. Podmínky pro odemykání
obsahu čteš jako `investigation.found_documents.contains(&DocumentId::Report2)`.

### `Document` — data, ne kód

Dokumenty (výsledková listina, hlášení StB, pohlednice, …) jsou **data**,
stejně jako `.veh`. Žádný dokument se nehardcoduje do Rust logiky —
obsah leží v `archive/data/documents.ron` (nebo JSON, konzistentně s
`serde_json` co už používáš ve `veh.rs`).

```rust
#[derive(Deserialize)]
struct DocumentDef {
    id: DocumentId,
    title: String,
    image: Option<String>,     // cesta k assetu (sken), pokud existuje
    body: String,               // text dokumentu
    redacted: bool,             // vizuální černé pruhy (viz styl v SS7_pribeh.md)
    unlocks: Vec<DocumentId>,   // co se odemkne po přečtení (volitelné)
}
```

Přidávání nového dokumentu = nový záznam v `.ron`, ne nový Rust kód. Sedí
to na to, jak je zapsaný seznam 10 dokumentů v `SS7_pribeh.md` — každý
se stane jedním záznamem.

### `Dialogue` — jednoduché větvení s podmínkami

Rozhovory (mechanik v aktu 1, Holoubek v aktu 3) jsou lineární s
občasnou volbou, ne plný dialogový strom jako v RPG. Návrh:

```rust
#[derive(Deserialize)]
struct DialogueLine {
    speaker: String,
    text: String,
    requires: Option<Flag>,       // řádek se ukáže jen když je flag nastavený
    sets: Option<Flag>,           // po přehrání nastaví flag
    pause_after_ms: u32,          // odmlky jsou designový prvek (viz styl doc)
}
```

Holoubkův rozhovor (akt 3) bude nejdelší datový soubor — ale pořád jen
sekvence `DialogueLine` s podmínkami, žádný nový systém navíc.

## 4. UI

Dvě obrazovky navíc k tomu, co už `main.rs` má (HUD, spawn menu):

- **Document viewer** — fullscreen overlay, obrázek/text dokumentu,
  redigovaná jména jako černé obdélníky přes text (ne `[REDACTED]`
  string). Bevy UI (`Node`, `ImageNode`, `Text`) stačí, není potřeba
  `bevy_egui`.
- **Dialogue box** — spodní panel, jeden `DialogueLine` v čase, klávesa
  pro pokračování. Odmlky (`pause_after_ms`) = systém co čeká, ne
  animace.

Volitelně, pokud se ukáže potřeba: jednoduchá **evidence board**
obrazovka (seznam nalezených dokumentů, žádné propojování myší — to by
byl samostatný scope navíc, zatím zbytečný).

## 5. Pohyb mezi lokacemi

Nejjednodušší varianta na start: **žádné auto, přímý výběr lokace**
(mapa/seznam bodů, klik → načte scénu). Auto jako doprava je hezký
detail, ale není nutný pro první průchod (akt 1) a přidává závislost na
`world.rs`. Navrhovaný postup:

1. **V1 (vertikální řez aktu 1):** lokace = statická scéna + dokument/
   dialog, přechod mezi nimi přes menu, žádné řízení vozidla.
2. **V2 (pokud to atmosféře pomůže):** jednoduchý walk/drive controller
   nezávislý na `SoftBody` — kinematický pohyb (`Transform` posouvaný
   přímo podle vstupu, žádná fyzika), případně nad `Terrain` z
   `world.rs` pro výšku terénu pod koly. Žádný Pacejka, žádné beamy.

Rozhodnutí V1 vs. V2 odložit, dokud nebude hotový obsah aktu 1 — psaní
dokumentů a dialogů je větší část práce než přesun mezi scénami.

## 6. Pořadí implementace (návrh)

1. `AppState` enum + gate existujících Rally systémů + prázdná `Archive`
   scéna (jen aby se dalo přepnout a vidět černou obrazovku).
2. `Investigation` resource (prázdný, jen kostra).
3. `DocumentDef` + loader z `.ron`, `document_viewer` UI systém — natvrdo
   3 dokumenty z aktu 1 (výsledková listina, přihláška, výstřižek).
4. `DialogueLine` systém — jeden krátký rozhovor (mechanik, akt 1) jako
   ověření, že větvení/flagy fungují.
5. Doplnit zbytek aktu 1 → akt 2 dokumenty → akt 3 (Holoubek, nejdelší
   dialog) → závěrečné artefakty podle `holoubek_pressure`.
6. Až funguje celý obsahový tok: audio (nahrávky), lepší UI, případně
   pohyb mezi lokacemi (V2).

Krok 1–4 je smysluplný vertikální řez — po něm uvidíš, jestli datový
formát (`.ron` dokumenty, `DialogueLine`) sedí na psaní zbytku obsahu,
než napíšeš všech 10 dokumentů a dlouhý Holoubkův rozhovor.

## 7. Co záměrně NENÍ v scope (zatím)

- Žádné ukládání/loadování stavu vyšetřování mezi spuštěními hry —
  `Investigation` žije jen v runtime. Přidat až bude jasné, že mode má
  smysl hrát na víc sezení.
- Žádná evidence board s propojováním (viz bod 4) — jen pokud se ukáže,
  že lineární tok nestačí.
- Žádné napojení na Steam/achievementy (fáze 6) — mode musí nejdřív
  fungovat samostatně.
