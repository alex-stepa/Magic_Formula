// jbeam.rs — převod BeamNG .jbeam textu na validní JSON.
//
// .jbeam = skoro JSON, ale: má komentáře (// a /* */) a často mezi prvky CHYBÍ čárky.
// Tady to opravíme, aby výsledek spolkl serde_json. Pracujeme po BAJTECH a kopírujeme
// je beze změny, takže UTF-8 (např. v textech) zůstane v pořádku — ASCII tokeny
// (závorky, čárky) mají bajty < 128, takže se nepletou s vícebajtovými znaky.

use crate::veh::{Beam, Node, Triangle, Wheel};

/// Vytáhne TROJÚHELNÍKY (povrch) — řádek je ["id1","id2","id3"]. Hlavičku ["id1:","id2:","id3:"]
/// přeskočíme (skip(1)), {modifikátory} vypadnou na as_array().
pub fn extract_triangles(root: &Value) -> Vec<Triangle> {
    let mut tris = Vec::new();
    let Some(parts) = root.as_object() else {
        return tris;
    };
    for part in parts.values() {
        let Some(rows) = part.get("triangles").and_then(|n| n.as_array()) else {
            continue;
        };
        for row in rows.iter().skip(1) {
            let Some(arr) = row.as_array() else {
                continue;
            };
            let (Some(a), Some(b), Some(c)) = (
                arr.first().and_then(|v| v.as_str()),
                arr.get(1).and_then(|v| v.as_str()),
                arr.get(2).and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            tris.push(Triangle {
                a: a.to_string(),
                b: b.to_string(),
                c: c.to_string(),
            });
        }
    }
    tris
}
use serde_json::Value;

/// Projde naparsovaný jbeam (strom Value) a vytáhne NODY ze všech partů,
/// co mají sekci "nodes". Bere řádky tvaru ["id", x, y, z, ...].
/// `{nodeWeight: ...}` je modifikátor — platí pro všechny NÁSLEDUJÍCÍ nody,
/// dokud ho jiný nezmění (stavový automat).
pub fn extract_nodes(root: &Value) -> Vec<Node> {
    let mut nodes = Vec::new();
    let Some(parts) = root.as_object() else {
        return nodes;
    };
    for part in parts.values() {
        let Some(rows) = part.get("nodes").and_then(|n| n.as_array()) else {
            continue;
        };
        let mut current_mass = 1.0_f64; // aktuální hmotnost z modifikátoru nodeWeight
        for row in rows.iter().skip(1) {
            // skip(1) = přeskoč hlavičku ["id","posX","posY","posZ"]

            // Modifikátor {…}: pokud nastavuje nodeWeight, zapamatuj si ho pro další nody.
            if let Some(obj) = row.as_object() {
                if let Some(w) = obj.get("nodeWeight").and_then(|v| v.as_f64()) {
                    current_mass = w;
                }
                continue;
            }

            let Some(arr) = row.as_array() else {
                continue;
            };
            let (Some(id), Some(x), Some(y), Some(z)) = (
                arr.first().and_then(|v| v.as_str()),
                arr.get(1).and_then(|v| v.as_f64()),
                arr.get(2).and_then(|v| v.as_f64()),
                arr.get(3).and_then(|v| v.as_f64()),
            ) else {
                continue;
            };
            nodes.push(Node {
                id: id.to_string(),
                pos: [x as f32, y as f32, z as f32],
                mass: current_mass as f32,
            });
        }
    }
    nodes
}

/// Vytáhne BEAMY ze všech partů, co mají sekci "beams".
/// Řádek dat je ["id1", "id2", ...]. Hlavičku ["id1:","id2:"] musíme přeskočit
/// EXPLICITNĚ (skip(1)) — je to taky pole stringů, takže by jinak prošla jako beam.
pub fn extract_beams(root: &Value) -> Vec<Beam> {
    let mut beams = Vec::new();
    let Some(parts) = root.as_object() else {
        return beams;
    };
    for part in parts.values() {
        let Some(rows) = part.get("beams").and_then(|n| n.as_array()) else {
            continue;
        };
        // Aktuální "materiál" beamu — modifikátory ho mění pro NÁSLEDUJÍCÍ beamy.
        let mut stiffness = 0.0_f64;
        let mut damping = 0.0_f64;
        for row in rows.iter().skip(1) {
            // Modifikátor {…}: pokud nastavuje beamSpring/beamDamp, zapamatuj si je.
            if let Some(obj) = row.as_object() {
                if let Some(s) = obj.get("beamSpring").and_then(|v| v.as_f64()) {
                    stiffness = s;
                }
                if let Some(d) = obj.get("beamDamp").and_then(|v| v.as_f64()) {
                    damping = d;
                }
                continue;
            }
            let Some(arr) = row.as_array() else {
                continue;
            };
            let (Some(a), Some(b)) = (
                arr.first().and_then(|v| v.as_str()),
                arr.get(1).and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            beams.push(Beam {
                a: a.to_string(),
                b: b.to_string(),
                stiffness: stiffness as f32,
                damping: damping as f32,
            });
        }
    }
    beams
}

/// Vytáhne KOLA ze všech partů, co mají sekci "pressureWheels".
/// Řádek dat: ["name","hubGroup","group","node1","node2","nodeS","nodeArm","wheelDir", {…}].
/// Bereme sloupce 0 (name), 3 (node1), 4 (node2), 7 (wheelDir). Hlavičku přeskočíme
/// (skip(1)), {modifikátory} taky (nejsou pole).
pub fn extract_wheels(root: &Value) -> Vec<Wheel> {
    let mut wheels = Vec::new();
    let Some(parts) = root.as_object() else {
        return wheels;
    };
    for part in parts.values() {
        let Some(rows) = part.get("pressureWheels").and_then(|n| n.as_array()) else {
            continue;
        };
        for row in rows.iter().skip(1) {
            let Some(arr) = row.as_array() else {
                continue; // {modifikátory} přeskoč
            };
            let (Some(name), Some(node1), Some(node2), Some(dir)) = (
                arr.first().and_then(|v| v.as_str()),
                arr.get(3).and_then(|v| v.as_str()),
                arr.get(4).and_then(|v| v.as_str()),
                arr.get(7).and_then(|v| v.as_i64()),
            ) else {
                continue;
            };
            wheels.push(Wheel {
                name: name.to_string(),
                node1: node1.to_string(),
                node2: node2.to_string(),
                dir: dir as i32,
            });
        }
    }
    wheels
}

/// Hlavní funkce: .jbeam text → validní JSON text.
pub fn to_json(input: &str) -> String {
    let step1 = strip_comments_and_insert_commas(input);
    // Obalit { } MUSÍ být PŘED odstraněním trailing čárek — jinak by čárka na konci
    // souboru (před nově přidanou }) zůstala a serde_json by ji odmítl.
    let wrapped = if step1.trim_start().starts_with('{') {
        step1
    } else {
        format!("{{{step1}}}")
    };
    remove_trailing_commas(&wrapped)
}

/// 1) smaže komentáře (mimo uvozovky)
/// 2) vloží chybějící čárku tam, kde po konci hodnoty hned začíná další hodnota
fn strip_comments_and_insert_commas(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut last_sig: u8 = 0; // poslední VÝZNAMNÝ (nebílý) bajt v out; 0 = zatím žádný
    let mut gap = true; // byl od posledního významného znaku oddělovač (mezera/komentář)?

    while i < bytes.len() {
        let c = bytes[i];

        // Řádkový komentář // ... → přeskoč do konce řádku (komentář je oddělovač).
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            gap = true;
            continue;
        }
        // Blokový komentář /* ... */
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            gap = true;
            continue;
        }

        // Bílé znaky: zkopíruj, oddělovač.
        if c.is_ascii_whitespace() {
            out.push(c);
            gap = true;
            i += 1;
            continue;
        }

        // Tady začíná VÝZNAMNÝ token. Čárku vložíme jen pokud:
        //   - byl mezi tímhle a předchozím tokenem oddělovač (gap) — jinak by se to
        //     vkládalo i mezi číslice jednoho čísla (4200) nebo písmena `true`,
        //   - tenhle token začíná hodnotu A předchozí token hodnotu končil.
        // Čárku vložíme když: token končí hodnotu a další ji začíná, A buď
        //   - byla mezi nimi mezera (gap), NEBO
        //   - je to JEDNOZNAČNÝ přechod (uvozovka/závorka), který nemůže být uprostřed
        //     tokenu — třeba `0.4"flashPoint"` nebo `}{`. (U dvou alfanum znaků bez mezery
        //     to NEVKLÁDÁME, jinak bychom rozbili číslo `1e5` nebo `4200`.)
        let unambiguous = matches!(c, b'"' | b'{' | b'[')
            || matches!(last_sig, b'}' | b']' | b'"');
        if (gap || unambiguous) && starts_value(c) && ends_value(last_sig) {
            out.push(b',');
        }
        gap = false;

        // Řetězec: zkopírujeme celý včetně uvozovek; \" je escapovaná uvozovka, ne konec.
        if c == b'"' {
            out.push(c);
            i += 1;
            while i < bytes.len() {
                let d = bytes[i];
                out.push(d);
                if d == b'\\' && i + 1 < bytes.len() {
                    out.push(bytes[i + 1]); // zkopíruj i escapovaný znak
                    i += 2;
                    continue;
                }
                i += 1;
                if d == b'"' {
                    break; // konec řetězce
                }
            }
            last_sig = b'"';
            continue;
        }

        out.push(c);
        last_sig = c;
        i += 1;
    }

    String::from_utf8(out).expect("vstup byl UTF-8, výstup taky")
}

/// Začíná tenhle bajt nějakou hodnotu? ({ [ " číslo  -  +  .  písmeno(true/false/null))
fn starts_value(c: u8) -> bool {
    matches!(c, b'{' | b'[' | b'"' | b'-' | b'+' | b'.') || c.is_ascii_digit() || c.is_ascii_alphabetic()
}

/// Končí tímhle bajtem nějaká hodnota? (} ] " číslo  písmeno)
fn ends_value(c: u8) -> bool {
    matches!(c, b'}' | b']' | b'"') || c.is_ascii_digit() || c.is_ascii_alphabetic()
}

/// Odstraní přebytečné čárky: čárku, za kterou (po bílých znacích) hned následuje ] nebo }.
/// Hlídá uvozovky, aby nesahala do čárek uvnitř textů.
fn remove_trailing_commas(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];

        // Řetězec zkopírujeme vcelku (čárky uvnitř textu se nesmí řešit).
        if c == b'"' {
            out.push(c);
            i += 1;
            while i < bytes.len() {
                let d = bytes[i];
                out.push(d);
                if d == b'\\' && i + 1 < bytes.len() {
                    out.push(bytes[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
                if d == b'"' {
                    break;
                }
            }
            continue;
        }

        if c == b',' {
            // koukni dopředu přes bílé znaky
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b']' || bytes[j] == b'}') {
                i += 1; // přebytečná čárka — přeskoč ji
                continue;
            }
        }

        out.push(c);
        i += 1;
    }

    String::from_utf8(out).expect("vstup byl UTF-8, výstup taky")
}
