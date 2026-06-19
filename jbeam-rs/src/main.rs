// jbeam-rs — konvertor BeamNG .jbeam → vlastní .veh formát.

mod jbeam; // preprocess .jbeam → validní JSON + extrakce nodů/beamů/kol
mod veh; // datové structy .veh formátu

use clap::Parser;
use std::path::PathBuf;

/// Konvertor BeamNG .jbeam → vlastní .veh formát.
#[derive(Parser)]
#[command(name = "jbeam-rs", about = "Konvertor BeamNG .jbeam → .veh")]
struct Args {
    /// Vstup: jeden .jbeam soubor, NEBO složka (sloučí všechny .jbeam v ní)
    input: PathBuf,

    /// Výstupní .veh soubor
    #[arg(short, long)]
    output: PathBuf,

    /// Jméno vozidla v .veh (výchozí = název vstupu)
    #[arg(short, long)]
    name: Option<String>,
}

fn main() {
    let args = Args::parse();

    // Sesbírej seznam .jbeam souborů: buď jeden soubor, nebo všechny ve složce.
    let files: Vec<PathBuf> = if args.input.is_dir() {
        std::fs::read_dir(&args.input)
            .expect("nepodařilo se přečíst složku")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "jbeam"))
            .collect()
    } else {
        vec![args.input.clone()]
    };

    // Posčítej nody/beamy/kola napříč všemi party.
    let mut nodes = Vec::new();
    let mut beams = Vec::new();
    let mut wheels = Vec::new();

    for file in &files {
        let raw = std::fs::read_to_string(file).expect("nepodařilo se přečíst soubor");
        let json = jbeam::to_json(&raw);
        // Pokud part nejde naparsovat, jen ho přeskočíme (ať nespadne celé sloučení).
        let value: serde_json::Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("přeskakuji {} ({e})", file.display());
                continue;
            }
        };
        nodes.extend(jbeam::extract_nodes(&value));
        beams.extend(jbeam::extract_beams(&value));
        wheels.extend(jbeam::extract_wheels(&value));
    }

    // Jméno auta: --name, jinak název vstupu (souboru/složky) bez přípony.
    let name = args.name.unwrap_or_else(|| {
        args.input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("vehicle")
            .to_string()
    });

    let vehicle = veh::Vehicle {
        name,
        version: 1,
        nodes,
        beams,
        wheels,
    };
    let out = serde_json::to_string_pretty(&vehicle).expect("serializace .veh selhala");
    std::fs::write(&args.output, &out).expect("zápis výstupního souboru selhal");

    println!(
        "Hotovo: {} souborů → {} ({} nodů, {} beamů, {} kol)",
        files.len(),
        args.output.display(),
        vehicle.nodes.len(),
        vehicle.beams.len(),
        vehicle.wheels.len()
    );
}
