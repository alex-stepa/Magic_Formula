# jbeam → .veh konvertor — replika logiky jbeam2veh (main.rs + jbeam.rs).
# SJSON parser zkopírován z "Beam proto2/tools/jbeam_to_toml.py" (import nejde —
# skript má top-level kód, co hned konvertuje).
import sys, json, os, math, re

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
        return list(parts.keys()), {}
    # index slotType → jména partů (fallback, když výchozí part chybí — je v BeamNG
    # common obsahu mimo mod; vezmeme první part modu se stejným slotType)
    by_type = {}
    for n, p in parts.items():
        st = p.get("slotType")
        if isinstance(st, str):
            by_type.setdefault(st, []).append(n)
    # offsets: part → nodeOffset ze slot řádku, co ho obsadil (posun nodů partu —
    # takhle BeamNG umísťuje kola: wheel part má lokální souřadnice, offset je na slotu)
    chosen, queue, missing, offsets = [], [(None, main, None)], [], {}
    seen = set()
    while queue:
        slot_type, name, off = queue.pop(0)
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
        if isinstance(off, dict):
            offsets[name] = off
        rows = parts[name].get("slots")
        if not isinstance(rows, list):
            continue
        for row in rows[1:]:
            if isinstance(row, list) and len(row) >= 2 and isinstance(row[0], str) and isinstance(row[1], str) and row[1]:
                mods = row[3] if len(row) >= 4 and isinstance(row[3], dict) else {}
                queue.append((row[0], row[1], mods.get("nodeOffset")))
    if missing:
        print(f"chybějící party bez náhrady (přeskočeny): {missing}", file=sys.stderr)
    return chosen, offsets

# Hodnota z jbeam: float rovnou; "$x" → default proměnné; "$=výraz" → mini-eval.
# Výrazy v datech: case(a,b,c), aritmetika, == nil, '' jako nil. Neznámé → None (ignoruj).
def resolve(val, jvars):
    if isinstance(val, float):
        return val
    if not isinstance(val, str):
        return None
    if val.startswith("$="):
        expr = val[2:]
    elif val.startswith("$"):
        return jvars.get(val[1:])
    else:
        return None
    expr = expr.replace("nil", "None").replace("''", "None")
    expr = re.sub(r"\$(\w+)", lambda m: repr(jvars.get(m.group(1))), expr)
    try:
        v = eval(expr, {"__builtins__": {}, "case": lambda c, a, b: a if c else b, "None": None}, {})
        return v if isinstance(v, float) else None
    except Exception:
        return None

def extract(root, offsets={}):
    nodes, beams, wheels, tris = [], [], [], []
    engine, gearbox, axles = None, {}, set()
    node_off = {}  # node id → nodeOffset partu (střed kola; wheel part má nody lokálně)
    tire_size = {}  # 'F'/'R' → (radius, width): tire part je v common obsahu mimo mod,
                    # ale VELIKOST nese jméno default tire slotu (tire_F_175_65_14_sport)
    if not isinstance(root, dict):
        return nodes, beams, wheels, tris, engine, gearbox, axles
    # Prepass: defaulty proměnných ze sekcí "variables" všech partů (řádek: [name, type,
    # unit, category, default, min, max, …]). Tuning party je mění, ale ty sloty neobsazují.
    jvars = {}
    for part in root.values():
        rows = part.get("variables") if isinstance(part, dict) else None
        if isinstance(rows, list):
            for row in rows[1:]:
                if isinstance(row, list) and len(row) >= 5 and isinstance(row[0], str):
                    jvars[row[0].lstrip("$")] = row[4]
    for pname, part in root.items():
        if not isinstance(part, dict):
            continue
        # nodeOffset ze slotu, co part obsadil: y/z floaty = použitelný střed.
        # x bývá $výraz (rozchod, "$=case($trackwidth == nil, $trackoffset+0.26, …)")
        # → resolve() ho z defaultů proměnných vyhodnotí; POZOR: je to OFFSET nodů
        # partu, absolutní střed kola se z něj skládá až v post-passu wheels níž.
        off = offsets.get(pname, {})
        center = None
        if isinstance(off.get("y"), float) and isinstance(off.get("z"), float):
            x = resolve(off.get("x"), jvars)
            center = [x if isinstance(x, float) else 0.0, off["y"], off["z"]]
        # TIRE SLOTY: z default jména (tire_F_175_65_14_sport) šířka/profil/ráfek
        # → radius = ráfek/2 + profil·šířka, width = šířka. Klíč F/R ze slotType.
        rows = part.get("slots")
        if isinstance(rows, list):
            for row in rows[1:]:
                if not (isinstance(row, list) and len(row) >= 2
                        and isinstance(row[0], str) and isinstance(row[1], str)
                        and row[0].startswith("tire")):
                    continue
                m = re.search(r"(\d{3})_(\d{2})_(\d{2})", row[1])
                if not m:
                    continue
                w_mm, aspect, rim_in = (float(g) for g in m.groups())
                axle = "F" if "_F" in row[0] else "R" if "_R" in row[0] else None
                tire_size[axle] = (rim_in * 0.0254 / 2 + aspect / 100 * w_mm / 1000,
                                   w_mm / 1000)
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
                    if center:
                        node_off[row[0]] = center
        # BEAMY: modifikátory {beamSpring/beamDamp/…} stavově + inline {…} v řádku (jen pro
        # ten řádek). Hodnoty umí být "$proměnná" nebo "$=výraz" → resolve. Předpětí
        # (beamPrecompression poměr / precompressionRange metry) se přepočte na rest níž.
        rows = part.get("beams")
        if isinstance(rows, list):
            k, d, dr, pre = 0.0, 0.0, 0.0, 1.0
            for row in rows[1:]:
                if isinstance(row, dict):
                    k = resolve(row.get("beamSpring"), jvars) if "beamSpring" in row else k
                    d = resolve(row.get("beamDamp"), jvars) if "beamDamp" in row else d
                    dr = resolve(row.get("beamDampRebound"), jvars) if "beamDampRebound" in row else dr
                    pre = resolve(row.get("beamPrecompression"), jvars) if "beamPrecompression" in row else pre
                    continue
                if not isinstance(row, list):
                    continue
                if len(row) >= 2 and isinstance(row[0], str) and isinstance(row[1], str):
                    inline = row[2] if len(row) > 2 and isinstance(row[2], dict) else {}
                    ki = resolve(inline.get("beamSpring"), jvars) if "beamSpring" in inline else k
                    di = resolve(inline.get("beamDamp"), jvars) if "beamDamp" in inline else d
                    dri = resolve(inline.get("beamDampRebound"), jvars) if "beamDampRebound" in inline else dr
                    pi = resolve(inline.get("beamPrecompression"), jvars) if "beamPrecompression" in inline else pre
                    # range (metry) jen inline — v datech se stavově nepoužívá; přebíjí poměr
                    ri = resolve(inline.get("precompressionRange"), jvars)
                    beams.append({"a": row[0], "b": row[1], "stiffness": ki or 0.0, "damping": di or 0.0,
                                  # rebound jen když je (0 = symetrické) — ať soubor nebobtná
                                  **({"rebound": dri} if dri else {}),
                                  "_pre": pi, "_rng": ri})
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
        # MOTOR: sekce "mainEngine" — torque tabulka [rpm, Nm] (bez hlavičky), volnoběh, omezovač
        # (sekci mainEngine mají i tuning party — ECU — jen s modifikátory;
        # motor je jen part s torque tabulkou, jinak by poslední part data smazal)
        me = part.get("mainEngine")
        if isinstance(me, dict) and isinstance(me.get("torque"), list):
            curve = [[r[0], r[1]] for r in me["torque"][1:]
                     if isinstance(r, list) and len(r) >= 2 and all(isinstance(v, float) for v in r[:2])]
            if curve:
                engine = {"torque_curve": curve,
                          "idle_rpm": me.get("idleRPM", 800.0),
                          "max_rpm": me.get("maxRPM", 7000.0)}
        # PŘEVODOVKA: sekce "gearbox" — gearRatios[0]=zpátečka, [1]=neutrál → bereme jen dopředné
        gb = part.get("gearbox")
        if isinstance(gb, dict) and isinstance(gb.get("gearRatios"), list):
            gearbox["ratios"] = [r for r in gb["gearRatios"][2:] if isinstance(r, float)]
        # FINÁL: kterákoli sekce partu s float "gearRatio" (finaldrive party) — poslední vyhrává.
        # String hodnoty ("$finaldrive_F" = jbeam proměnná) se přeskočí.
        for sec in part.values():
            if isinstance(sec, dict) and isinstance(sec.get("gearRatio"), float):
                gearbox["final_drive"] = sec["gearRatio"]
        # POHON: powertrain řádky ["differential", "differential_F", …] = hnané osy.
        # Center diff / transfercase se jmenuje jinak → nevadí, stačí F/R nápravy.
        rows = part.get("powertrain")
        if isinstance(rows, list):
            for row in rows[1:]:
                if isinstance(row, list) and len(row) >= 2 and row[0] == "differential" and isinstance(row[1], str):
                    if row[1].endswith("_F"):
                        axles.add("F")
                    if row[1].endswith("_R"):
                        axles.add("R")
    posd = {n["id"]: n["pos"] for n in nodes}
    # Střed kola = nodeOffset partu, kterému patří jeho axle nody (pressureWheels
    # bývají v jiném partu — wheeldata — než nody kola). x nodeOffsetu je OFFSET
    # lokálních nodů partu → absolutní |x| středu = průměr |x| axle nodů + offset.
    for w in wheels:
        c = node_off.get(w["node1"]) or node_off.get(w["node2"])
        if c:
            pa, pb = posd.get(w["node1"]), posd.get(w["node2"])
            x = (abs(pa[0]) + abs(pb[0])) / 2 + c[0] if pa and pb else c[0]
            w["center"] = [round(x, 6), c[1], c[2]]
        # Rozměr pneu z tire slotu, jen když ho pressureWheels nedaly (radius 0).
        # Náprava podle y středu (BeamNG +y = dozadu → přední y < 0).
        if w["radius"] == 0.0 and tire_size:
            axle = ("F" if c[1] < 0 else "R") if c else None
            r_w = tire_size.get(axle) or next(iter(tire_size.values()))
            w["radius"], w["width"] = round(r_w[0], 6), r_w[1]
    # Předpětí → klidová délka beamu (pružiny drží statickou zátěž jen díky němu).
    # precompressionRange = metry navíc k délce; beamPrecompression = poměr délky.
    # Až tady — beam smí odkazovat na nody z partu zpracovaného později.
    for b in beams:
        pre, rng = b.pop("_pre", None), b.pop("_rng", None)
        pa, pb = posd.get(b["a"]), posd.get(b["b"])
        if pa is None or pb is None:
            continue
        length = math.dist(pa, pb)
        if isinstance(rng, float) and rng != 0.0:
            b["rest"] = round(length + rng, 6)
        elif isinstance(pre, float) and pre != 1.0:
            b["rest"] = round(length * pre, 6)
    return nodes, beams, wheels, tris, engine, gearbox, axles

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
    chosen, offsets = select_parts(parts)
    print(f"party: {len(parts)} celkem → {len(chosen)} vybraných sloty", file=sys.stderr)
    # 3) extrakce jen z vybraných partů
    nodes, beams, wheels, tris, engine, gearbox, axles = extract({n: parts[n] for n in chosen}, offsets)
    # dedup nodů podle id — poslední definice vyhrává (override jako v BeamNG)
    before = len(nodes)
    by_id = {}
    for n in nodes:
        by_id[n["id"]] = n
    nodes = list(by_id.values())
    print(f"nody: {before} → {len(nodes)} po deduplikaci", file=sys.stderr)
    vehicle = {"name": name, "version": 1, "nodes": nodes, "beams": beams, "wheels": wheels, "triangles": tris}
    if engine:
        vehicle["engine"] = engine
    if gearbox.get("ratios"):
        vehicle["gearbox"] = gearbox
    # Layout = serde externí tag pro hru: "fwd"/"rwd" string, AWD objekt s dělením momentu.
    if axles == {"F"}:
        vehicle["layout"] = "fwd"
    elif axles == {"R"}:
        vehicle["layout"] = "rwd"
    elif axles:
        vehicle["layout"] = {"awd": {"front": 0.5}}  # ponytail: split 50/50, reálný z center diffu až bude potřeba
    print(f"motor: {'ano' if engine else 'NE'}, převodovka: {gearbox or 'NE'}, pohon: {vehicle.get('layout', 'NE')}", file=sys.stderr)
    with open(out_path, "w") as f:
        json.dump(vehicle, f, indent=1)
    print(f"Hotovo: {len(files)} souborů → {out_path} ({len(nodes)} nodů, {len(beams)} beamů, {len(wheels)} kol, {len(tris)} trojúhelníků)")

if __name__ == "__main__":
    main()
