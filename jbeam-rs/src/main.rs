// jbeam-rs — konvertor BeamNG .jbeam → vlastní .veh formát.

mod jbeam; // preprocess .jbeam → validní JSON
mod veh; // datové structy .veh formátu

fn main() {
    // Zatím natvrdo testujeme na jednom partu; CLI přidáme později.
    let path = "samples/fabia/vehicles/fabia/fabia_body.jbeam";

    let raw = std::fs::read_to_string(path).expect("nepodařilo se přečíst soubor");
    let json = jbeam::to_json(&raw);

    // Naparsujeme do obecného stromu Value (ne pevných structů — vstup je nepravidelný).
    let value: serde_json::Value = serde_json::from_str(&json).expect("nevalidní JSON po preprocess");

    // Vytáhneme nody a beamy a poskládáme z nich vozidlo.
    let vehicle = veh::Vehicle {
        name: "fabia".to_string(),
        version: 1,
        nodes: jbeam::extract_nodes(&value),
        beams: jbeam::extract_beams(&value),
    };

    // Serializujeme do .veh (hezky odsazený JSON) a zapíšeme.
    let out = serde_json::to_string_pretty(&vehicle).expect("serializace .veh selhala");
    std::fs::write("fabia.veh", &out).expect("zápis fabia.veh selhal");
    println!(
        "Hotovo → fabia.veh ({} nodů, {} beamů)",
        vehicle.nodes.len(),
        vehicle.beams.len()
    );
}
