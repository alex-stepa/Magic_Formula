# jbeam → .veh konvertor — replika logiky jbeam2veh (main.rs + jbeam.rs).
# SJSON parser zkopírován z "Beam proto2/tools/jbeam_to_toml.py" (import nejde —
# skript má top-level kód, co hned konvertuje).
import sys, json, os

def tokenize(s):
    i, n = 0, len(s); toks = []
    while i < n:
        c = s[i]
        if c in ' \t\r\n,': i += 1; continue
        if c == '/' and i+1 < n and s[i+1] == '/':
            while i < n and s[i] != '\n': i += 1
            continue
        if c == '/' and i+1 < n and s[i+1] == '*':
            i += 2
            while i+1 < n and not (s[i] == '*' and s[i+1] == '/'): i += 1
            i += 2; continue
        if c in '{}[]:': toks.append(c); i += 1; continue
        if c == '"':
            i += 1; buf = []
            while i < n and s[i] != '"':
                if s[i] == '\\': buf.append(s[i+1]); i += 2
                else: buf.append(s[i]); i += 1
            i += 1; toks.append(('s', ''.join(buf))); continue
        j = i
        while i < n and s[i] not in ' \t\r\n,{}[]:"/': i += 1
        word = s[j:i]
        try: toks.append(('n', float(word)))
        except ValueError:
            if word in ('true', 'false', 'null'): toks.append(('b', {'true': True, 'false': False, 'null': None}[word]))
            else: toks.append(('s', word))
    return toks

class P:
    def __init__(self, toks): self.t = toks; self.i = 0
    def parse(self): return self.val()
    def val(self):
        tok = self.t[self.i]
        if tok == '{': return self.obj()
        if tok == '[': return self.arr()
        self.i += 1
        return tok[1]
    def obj(self):
        self.i += 1; d = {}
        while self.t[self.i] != '}':
            k = self.t[self.i]; self.i += 1
            kk = k[1] if isinstance(k, tuple) else k
            if self.i < len(self.t) and self.t[self.i] == ':': self.i += 1
            d[kk] = self.val()
        self.i += 1; return d
    def arr(self):
        self.i += 1; a = []
        while self.t[self.i] != ']':
            a.append(self.val())
        self.i += 1; return a

def load(path):
    text = open(path).read()
    # jbeam party často nemají vnější {} — obal je (stejně jako jbeam2veh to_json)
    if not text.lstrip().startswith('{'):
        text = '{' + text + '}'
    return P(tokenize(text)).parse()

# Výběr partů podle SLOTŮ (jako BeamNG): hlavní part (slotType=="main") rekurzivně
# obsazuje sloty výchozími party. Party mimo strom (alternativní vzpěry, skiny…) se zahodí.
# Řádek slotu: [slotType, výchozí_part_name, popis, {modifikátory}]; prázdný default = prázdný slot.
def select_parts(parts):
    main = next((n for n, p in parts.items() if p.get("slotType") == "main"), None)
    if main is None:
        print("žádný part se slotType=main — beru všechny (staré chování)", file=sys.stderr)
        return list(parts.keys())
    # index slotType → jména partů (fallback, když výchozí part chybí — je v BeamNG
    # common obsahu mimo mod; vezmeme první part modu se stejným slotType)
    by_type = {}
    for n, p in parts.items():
        st = p.get("slotType")
        if isinstance(st, str):
            by_type.setdefault(st, []).append(n)
    chosen, queue, missing = [], [(None, main)], []
    seen = set()
    while queue:
        slot_type, name = queue.pop(0)
        if name not in parts:
            alts = sorted(by_type.get(slot_type, []))
            if alts:
                print(f"part '{name}' chybí, slot {slot_type} obsazen '{alts[0]}'", file=sys.stderr)
                name = alts[0]
            else:
                missing.append(name)
                continue
        if name in seen:
            continue
        seen.add(name)
        chosen.append(name)
        rows = parts[name].get("slots")
        if not isinstance(rows, list):
            continue
        for row in rows[1:]:
            if isinstance(row, list) and len(row) >= 2 and isinstance(row[0], str) and isinstance(row[1], str) and row[1]:
                queue.append((row[0], row[1]))
    if missing:
        print(f"chybějící party bez náhrady (přeskočeny): {missing}", file=sys.stderr)
    return chosen

def extract(root):
    nodes, beams, wheels, tris = [], [], [], []
    if not isinstance(root, dict):
        return nodes, beams, wheels, tris
    for part in root.values():
        if not isinstance(part, dict):
            continue
        # NODY: modifikátor {nodeWeight} platí pro následující řádky (stavový automat)
        rows = part.get("nodes")
        if isinstance(rows, list):
            mass = 1.0
            for row in rows[1:]:  # [0] = hlavička
                if isinstance(row, dict):
                    if isinstance(row.get("nodeWeight"), float):
                        mass = row["nodeWeight"]
                    continue
                if not isinstance(row, list):
                    continue
                if len(row) >= 4 and isinstance(row[0], str) and all(isinstance(v, float) for v in row[1:4]):
                    nodes.append({"id": row[0], "pos": [row[1], row[2], row[3]], "mass": mass})
        # BEAMY: modifikátory {beamSpring/beamDamp} stavově, inline {…} v řádku se ignorují
        rows = part.get("beams")
        if isinstance(rows, list):
            k, d = 0.0, 0.0
            for row in rows[1:]:
                if isinstance(row, dict):
                    if isinstance(row.get("beamSpring"), float):
                        k = row["beamSpring"]
                    if isinstance(row.get("beamDamp"), float):
                        d = row["beamDamp"]
                    continue
                if not isinstance(row, list):
                    continue
                if len(row) >= 2 and isinstance(row[0], str) and isinstance(row[1], str):
                    beams.append({"a": row[0], "b": row[1], "stiffness": k, "damping": d})
        # KOLA: pressureWheels, sloupce 0/3/4/7 = name/node1/node2/wheelDir.
        # Modifikátory radius/tireWidth stavově (jako beamSpring) — tenhle mod je nemá
        # (tire party jsou v BeamNG common obsahu), pak zůstane 0 a hra si dá fallback.
        rows = part.get("pressureWheels")
        if isinstance(rows, list):
            radius, width = 0.0, 0.0
            for row in rows[1:]:
                if isinstance(row, dict):
                    if isinstance(row.get("radius"), float):
                        radius = row["radius"]
                    if isinstance(row.get("tireWidth"), float):
                        width = row["tireWidth"]
                    continue
                if not isinstance(row, list) or len(row) < 8:
                    continue
                if isinstance(row[0], str) and isinstance(row[3], str) and isinstance(row[4], str) and isinstance(row[7], float):
                    wheels.append({"name": row[0], "node1": row[3], "node2": row[4], "dir": int(row[7]),
                                   "radius": radius, "width": width})
        # TROJÚHELNÍKY: řádky ["a","b","c"]
        rows = part.get("triangles")
        if isinstance(rows, list):
            for row in rows[1:]:
                if isinstance(row, list) and len(row) >= 3 and all(isinstance(v, str) for v in row[:3]):
                    tris.append({"a": row[0], "b": row[1], "c": row[2]})
    return nodes, beams, wheels, tris

def main():
    src_dir, out_path, name = sys.argv[1], sys.argv[2], sys.argv[3]
    files = sorted(f for f in os.listdir(src_dir) if f.endswith(".jbeam"))
    # 1) posbírej VŠECHNY party ze všech souborů (jméno partu = klíč v root objektu)
    parts = {}
    for f in files:
        try:
            root = load(os.path.join(src_dir, f))
        except Exception as e:
            print(f"přeskakuji {f} ({e})", file=sys.stderr)
            continue
        if isinstance(root, dict):
            for pname, part in root.items():
                if isinstance(part, dict):
                    parts[pname] = part
    # 2) sloty: vyber jen party dosažitelné z hlavního partu přes výchozí hodnoty slotů
    chosen = select_parts(parts)
    print(f"party: {len(parts)} celkem → {len(chosen)} vybraných sloty", file=sys.stderr)
    # 3) extrakce jen z vybraných partů
    nodes, beams, wheels, tris = extract({n: parts[n] for n in chosen})
    # dedup nodů podle id — poslední definice vyhrává (override jako v BeamNG)
    before = len(nodes)
    by_id = {}
    for n in nodes:
        by_id[n["id"]] = n
    nodes = list(by_id.values())
    print(f"nody: {before} → {len(nodes)} po deduplikaci", file=sys.stderr)
    vehicle = {"name": name, "version": 1, "nodes": nodes, "beams": beams, "wheels": wheels, "triangles": tris}
    with open(out_path, "w") as f:
        json.dump(vehicle, f, indent=1)
    print(f"Hotovo: {len(files)} souborů → {out_path} ({len(nodes)} nodů, {len(beams)} beamů, {len(wheels)} kol, {len(tris)} trojúhelníků)")

if __name__ == "__main__":
    main()
