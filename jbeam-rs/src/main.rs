// jbeam-rs — konvertor BeamNG .jbeam → vlastní .veh formát.

mod jbeam; // preprocess .jbeam → validní JSON + extrakce nodů/beamů
mod veh; // datové structy .veh formátu

use clap::Parser;
use std::path::PathBuf;

/// Konvertor BeamNG .jbeam → vlastní .veh formát.
/// clap z tohohle structu sám vyrobí parsování argumentů i `--help`.
#[derive(Parser)]
#[command(name = "jbeam-rs", about = "Konvertor BeamNG .jbeam → .veh")]
struct Args {
    /// Vstupní .jbeam soubor
    input: PathBuf,

    /// Výstupní .veh soubor
    #[arg(short, long)]
    output: PathBuf,

    /// Jméno vozidla v .veh (výchozí = název vstupního souboru)
    #[arg(short, long)]
    name: Option<String>,
}

fn main() {
    let args = Args::parse();

    // Načti a převeď .jbeam na validní JSON, pak na strom Value.
    let raw = std::fs::read_to_string(&args.input).expect("nepodařilo se přečíst vstupní soubor");
    let json = jbeam::to_json(&raw);
    let value: serde_json::Value = serde_json::from_str(&json).expect("nevalidní JSON po preprocess");

    // Jméno auta: buď z --name, nebo z názvu souboru (bez přípony).
    let name = args.name.unwrap_or_else(|| {
        args.input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("vehicle")
            .to_string()
    });

    // Poskládej vozidlo a zapiš .veh.
    let vehicle = veh::Vehicle {
        name,
        version: 1,
        nodes: jbeam::extract_nodes(&value),
        beams: jbeam::extract_beams(&value),
    };
    let out = serde_json::to_string_pretty(&vehicle).expect("serializace .veh selhala");
    std::fs::write(&args.output, &out).expect("zápis výstupního souboru selhal");

    println!(
        "Hotovo: {} → {} ({} nodů, {} beamů)",
        args.input.display(),
        args.output.display(),
        vehicle.nodes.len(),
        vehicle.beams.len()
    );
}
