# SS7 — Holoubkův rozhovor (plné znění, akt 3)

Doplňuje ukázku z [`SS7_dokumenty.md`](SS7_dokumenty.md), dokument 9.
Hlavní scéna celého gamemode. Formát odpovídá `DialogueLine` z
[`SS7_technicka.md`](SS7_technicka.md) — `speaker`, `text`, odmlky jako
designový prvek, větvení přes `requires`/`sets`.

Scéna má **tři místa, kde hráč může zvolit "tlačit" nebo "počkat"**.
Součet zvolených tlaků = `holoubek_pressure` (0–3), který určuje, který
závěr hra ukáže — viz [`SS7_zaver_varianty.md`](SS7_zaver_varianty.md).

Prostředí: zahrada za domem, pozdní odpoledne, Holoubek sedí, hráč
stojí/sedí naproti. Žádná hudba, jen okolní zvuk (vítr, vzdálený pes).

---

## Úvod (navazuje na dokument 9)

> **Holoubek:** Vy jste ten, co se ptal Sýkory. Volal mi.
>
> **Hráč:** Ano.
>
> **Holoubek:** Čekal jsem, že se někdo jednou zeptá. Nečekal jsem, že
> to bude trvat skoro čtyřicet let. *(odmlka, 3s)* Sedněte si. Tohle
> nebude krátký.

## Část 1 — Karel

> **Holoubek:** Karel byl... *(hledá slovo)* ...přesnej. Ne rychlej za
> každou cenu, přesnej. Věděl, kam dá kolo, ještě než tam bylo. Jezdili
> jsme spolu pět sezón. Byl to nejlepší spolujezdec — teda, on byl
> jezdec, já spolujezdec, ale rozumíte mi.
>
> **Hráč:** Věděl jste, že má tetu v Rakousku?
>
> **Holoubek:** *(delší odmlka)* Věděl. Nikdy o tom nemluvil moc, ale
> věděl jsem. Psali si. Přes někoho, ne přímo, to by bylo... no, to
> byste tenkrát nedělal přímo.

**Volba (tlak #1):**

- **Tlačit:** *„A vy jste o tom hlásil, že jo?"*
  → Holoubek ztuhne, odpoví rychleji, obranněji. `sets: Flag::Pressure1`
- **Počkat:** ticho, necháš ho pokračovat sám.
  → pokračuje plynuleji, bez obrany.

### Větev — tlak

> **Holoubek:** *(ostřeji)* Jo. Hlásil. Chcete to slyšet takhle přímo?
> Tak jo — hlásil jsem. Ne všechno, ale hlásil.

### Větev — počkání

> **Holoubek:** *(sám, tišeji)* A já... já jsem o tom taky hlásil.
> Ne proto že bych chtěl. Musel jsem.

## Část 2 — Tlak StB

> **Holoubek:** Táta potřeboval na operaci do NDR, měli tam lepší
> pracoviště na tohleto. Bez povolení k vycestování by se nedostal.
> A povolení... *(odmlka)* ...povolení nebylo zadarmo. Přišel za mnou
> chlap, nikdy neřekl celý jméno, jen že je od bezpečnosti. Řekl, že
> stačí drobnosti. Nálady, s kým se Karel baví, jestli je nervózní.
> Nic velkýho, tak to řekl.
>
> **Hráč:** A bylo to jenom "nic velkýho"?
>
> **Holoubek:** Ze začátku jo. Pak se ptali víc. A já jsem si říkal, že
> když přestanu, táta na tu operaci nepojede. Tak jsem pokračoval.

**Volba (tlak #2):**

- **Tlačit:** *„Věděl jste, že ho tím sledujou kvůli emigraci?"*
  → přizná víc, ale s viditelnou bolestí, `sets: Flag::Pressure2`
- **Počkat:** necháš odmlku, on sám dojde k nejtěžší části.

### Větev — tlak

> **Holoubek:** *(potichu)* Tušil jsem, k čemu to směřuje. Neřekl jsem
> si to nahlas, ale tušil jsem. A pokračoval jsem stejně. To je to,
> co jsem si nesl nejhůř. Ne že jsem hlásil — že jsem tušil a hlásil
> dál.

### Větev — počkání

> **Holoubek:** *(po dlouhé odmlce)* Nejhorší na tom nebylo, že jsem
> hlásil. Nejhorší bylo, že jsem tušil, k čemu to je, a stejně jsem
> pokračoval.

## Část 3 — Ta noc

> **Holoubek:** Sedmá zkouška. Mlha jak podšívka, viděl jste na dvacet
> metrů, ne víc. Trasu tejden předtím přeložili, prej sesuv, ale jela
> teď blíž k hranici, to jsme si všimli oba. Karel byl ten den jinej.
> Klidnej. Až moc klidnej.
>
> Asi v polovině zkoušky zpomalil, i když to tam vůbec nebylo potřeba.
> Zastavil úplně. Řekl jen: *„zůstaň tady"*. Vystoupil, a šel. Pěšky,
> do lesa, tím směrem k hranici. Neřek nic víc. Neohlíd se.
>
> **Hráč:** A co jste udělal vy?
>
> **Holoubek:** Seděl jsem v tom autě přes hodinu. Pak jsem došel na
> nejbližší kontrolu pěšky a řekl, že jsme měli nehodu, že Karel je
> pryč, že jsem ho nenašel. Což... technicky nebyla lež. Jenom jsem
> neřekl všechno.

**Volba (tlak #3):**

- **Tlačit:** *„Proč jste to neřekl pravdu, ani později?"*
  → Holoubek se uzavře, odpoví krátce a definitivně, `sets: Flag::Pressure3`
- **Počkat:** necháš ho domluvit sám, bez nátlaku.

### Větev — tlak

> **Holoubek:** *(chladněji, poprvé v celém rozhovoru)* Protože kdybych
> řekl pravdu, oni by hledali dál. Jeho. Mě. Tátu. *(vstane)* Myslím,
> že jsme toho na dneska řekli dost.
>
> *(rozhovor končí zde — hráč nedostane část 4)*

### Větev — počkání

> **Holoubek:** Protože pravda by ho nezachránila, jenom by ohrozila
> nás ostatní. Nesl jsem tohle sám skoro čtyřicet let. Vy jste první,
> komu to říkám celý.

## Část 4 — pouze pokud hráč netlačil ve třetí volbě

> **Holoubek:** Jednu věc vám ještě povím. Před pár lety mi přišel
> dopis. Bez zpáteční adresy, jenom rakouská známka. Uvnitř fotka —
> muž, starší, u dílny, usmívá se. Na zadní straně nic. Žádný jméno.
>
> **Hráč:** Myslíte, že to byl on?
>
> **Holoubek:** Nevím jistě. Ale chci tomu věřit. A myslím, že po tolika
> letech si to věřit zasloužím.

---

## Poznámka k `holoubek_pressure`

- **0** (obě volby "počkat" + nedošlo k tlaku #3): hráč dostane celou
  část 4, nejvřelejší uzavření.
- **1–2** (nějaký tlak, ale ne ve třetí volbě): hráč dostane část 4,
  ale Holoubek je viditelně unavenější, tón chladnější.
- **3** (tlak i ve třetí volbě): rozhovor končí po části 3, žádná
  fotka, žádné potvrzení od Holoubka samotného — hráč musí uzavřít
  příběh jen z archivních důkazů (pohlednice, dokument 10).
