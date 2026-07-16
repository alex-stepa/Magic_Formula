# SS7 — "Etapa, která neexistuje"

Narativní gamemode, koncepčně i technicky oddělený od hlavní rally/freeroam hry.
Žádná závislost na `SoftBody`, drivetrain, tire modelu ani solveru — sdílí
maximálně render pipeline a případně `World`/terrain trait pro cestování
mezi lokacemi. Vlastní `AppState` (např. `AppState::Archive`), vlastní scéna.

Status: koncept, první průchod. Jména, místa a detaily jsou fiktivní.

---

## 1. Přehled

V roce 1987 se na **Rallye Podyjí** (fiktivní socialistický motoristický
podnik, Svazarm/AMK Znojmo, oblast jižní Moravy u rakouské hranice) ztrácí
posádka na sedmé rychlostní zkoušce — **SS7 „Vlčí důl"**. Auto se nikdy
nenašlo, tělo se nikdy nenašlo. V dalších ročnících už SS7 v route booku
není. Z oficiálních výsledků zmizela celá etapa, jako by se nikdy nejela.

V současnosti na tu nesrovnalost narazí **nadšenec historických rally** —
někdo, kdo skenuje archivy, sbírá staré route booky, jezdí po opuštěných
tratích. Najde díru v datech a začne pátrat. Zjistí, že za zmizením nestojí
nehoda, ale **StB**. A nakonec zjistí, že jezdec nezemřel — **přejel
hranici**.

Žádná nadpřirozená vrstva, žádný loop, žádná horor mechanika v tomhle
gamemode. Napětí stojí na archivním pátrání, redigovaných dokumentech,
lidech, co nechtějí mluvit, a postupném skládání pravdy. Atmosféra je blízká
found-footage / archivnímu hororu (viz. hlavní hra), ale bez nadpřirozena —
je to čistě lidský, historický thriller.

---

## 2. Klíčové postavy

**Karel Vrba** — jezdec, 29 let v roce 1987. Zkušený závodník AMK Znojmo,
technicky nadaný, potichu ambiciózní. Má tetu v Rakousku (emigrovala
koncem 60. let) a přes ni už rok udržuje tajný kontakt — dopisy přes
prostředníka, jednou i krátké setkání na mezinárodním podniku v Maďarsku.
Plánuje emigraci dlouho předtím, než se stane cokoliv na SS7 — rallye je
jen příležitost, ne impuls.

**Petr Holoubek** — spolujezdec, Karlův dlouholetý parťák. Pod tlakem StB
(otcova žádost o vycestování byla podmíněná spoluprací) donucen hlásit
Karlovo chování a kontakty. Není padouch — je to člověk v pasti, co dělá
minimum nutné k přežití vlastní rodiny. V roce 1987 je mu 27. V současné
době je mu přes 80, žije v Znojmě, je to **klíčový pamětník**, na kterého
hráč ve třetím aktu narazí.

**Kapitán StB (jméno v dokumentech redigováno, hráč ho nikdy nepozná plným
jménem)** — vede sledování Vrby. Nepřítomný jako postava, přítomný jen
skrz hlášení. Zosobňuje aparát, ne jednotlivce.

**Protagonista (hráč)** — bezejmenný/volitelně pojmenovaný současný
nadšenec. Sedí na pozici výzkumníka: skenuje dokumenty, jezdí po lokacích,
vede rozhovory.

---

## 3. Struktura příběhu — tři akty

### Akt 1 — Nesrovnalost

Hráč začíná s digitalizovaným archivem výsledků Rallye Podyjí z 80. let
(fiktivní, ale vzhled odpovídá reálným socialistickým sportovním
bulletinům). Všimne si, že ročník 1987 má v route booku SS7 „Vlčí důl",
ale ve výsledkové listině skáče rovnou ze SS6 na SS8. Přihláška posádky
Vrba/Holoubek existuje, výsledek žádný.

Cíl aktu: vzbudit otázku. Hráč sežene dobový regionální tisk — krátká
zpráva o „tragické nehodě" bez jmen, bez detailů, zarámovaná jako běžné
sportovní neštěstí. Podnikne první terénní výjezd na fyzické místo SS7 —
dnes zarostlá, neudržovaná polní cesta. Najde tam první fyzický artefakt
(rozbitý kilometrovník, zbytky start. čísla) — nic nadpřirozeného, jen
prach a čas.

Uzavření aktu: hráč získá kontakt na žijícího pamětníka (jiný mechanik z
té doby, ne Holoubek — ten se objeví až v aktu 3), který mu při krátkém
rozhovoru řekne jen tolik, aby to bylo podezřelé — odmítne mluvit o
detailech, ale prořekne se, že "na to se ptali i tenkrát, akorát v
uniformách".

### Akt 2 — Stopa vede k StB

Hráč se dostane (fiktivní obdoba ÚSTR — reálná instituce v ČR skutečně
zpřístupňuje svazky StB veřejnosti, takže tahle výzkumná mechanika má
autentický základ) k částečně redigovaným hlášením. Klíčová zjištění,
postupně:

1. Vrba byl sledován kvůli **"kontaktu se zahraničím"** — konkrétně teta v
   Rakousku.
2. Existuje **spolupracovník s krycím jménem**, který dodával průběžná
   hlášení o Vrbově chování, náladách, plánech.
3. Trasa SS7 byla **týden před podnikem přeložena** blíž k hraničnímu
   pásmu — oficiálně kvůli sesuvu na původní trati, ale dokument
   naznačuje, že o změně rozhodovalo jiné oddělení než organizátoři.

Hráč zatím neví, kdo je informátor. Dokumenty jsou schválně
neúplné/rozporuplné — jeden fragment hlášení naznačuje, že informátor byl
"blízko posádce", ne nutně člen. Tohle je moment, kde si hráč může
domýšlet (mechanik? funkcionář týmu? spolujezdec?) — odpověď přijde až v
aktu 3.

Uzavření aktu: hráč najde v archivu **nedoručenou pohlednici** — fragment
adresovaný Vrbově matce, orazítkovaný v Rakousku, datovaný pár týdnů po
SS7, zabavený a nikdy nedoručený, uložený jako "důkazní materiál" ve
stejné složce jako hlášení. To je první tvrdý důkaz, že Vrba **přežil**.

### Akt 3 — Petr Holoubek

Hráč dohledá, že spolujezdec Petr Holoubek pořád žije, dnes přes
osmdesát, v Znojmě. Rozhovor je jádrem celého gamemode — dlouhý, těžký,
odehrává se v jeho bytě/zahradě, ne v archivu. Holoubek nejdřív odmítá
mluvit (stará reflexivní opatrnost), postupně, s tíhou, řekne pravdu:

- Ano, věděl o Vrbově plánu — ne detaily, ale tušil.
- Ano, hlásil StB drobnosti, protože jinak by jeho otec nikdy nedostal
  povolení vycestovat na operaci do NDR.
- Tu noc na SS7, v mlze, Vrba záměrně vyjel z trati na místě, které znal
  z rekognoskace — místo, kde silnice vede nejblíž k hraničnímu pásmu.
  Zastavil auto, řekl Holoubkovi jen "zůstaň", a zmizel pěšky do lesa.
- Holoubek nahlásil "nehodu" — což technicky nebyla lež, jen neúplná
  pravda — a nikdy neřekl víc, ani když ho vyslýchali. Posledních skoro
  40 let s tím žije.

Uzavření aktu / resolution: hráč s Holoubkovým svolením (nebo bez něj,
podle větve) dohledá další stopu — matrika, rakouský archiv, případně
žijící potomek Vrby v Rakousku dnes. Konec je tichý, ne velkolepý:
potvrzení, že Vrba žil dál, možná fotka ze svatby, nekrolog z 2010s
(zemřel přirozeně, starý, v Rakousku), nebo přímý kontakt s jeho dcerou/
synem, co o otcově rallye minulosti nikdy nic nevěděli.

---

## 4. Seznam dokumentů a nahrávek (pořadí objevování)

1. **Výsledková listina 1987** — skenovaný bulletin, SS6 → SS8, žádná SS7.
2. **Přihláška posádky** — Vrba/Holoubek, Škoda 130 LR, startovní číslo.
3. **Novinový výstřižek** — krátká zpráva o "nehodě", bez jmen.
4. **Rozhovor #1 — mechanik** (audio nebo text) — vyhýbavý, jedna
   prořeknutá věta jako hook do aktu 2.
5. **Redigované hlášení StB #1** — sledování kvůli "kontaktu se
   zahraničím", jména začerněná kromě Vrbova.
6. **Redigované hlášení StB #2** — zmínka o spolupracovníkovi s krycím
   jménem, kontext naznačuje blízkost k posádce.
7. **Původní vs. revidovaný route book** — trasa SS7 přeložená týden
   před podnikem, důvod "sesuv půdy" podtržený jiným rukopisem.
8. **Nedoručená pohlednice** — orazítkovaná v Rakousku, adresovaná
   Vrbově matce, zabavená.
9. **Rozhovor #2 — Petr Holoubek** (hlavní scéna aktu 3, dlouhá,
   vícedílná, s pauzami/odmlkami jako součást psaní dialogu). Plné
   znění: [`SS7_holoubek_rozhovor.md`](SS7_holoubek_rozhovor.md).
10. **Závěrečný artefakt** — matrika/nekrolog/fotka/kontakt s potomkem —
    varianta podle toho, jak moc hráč v aktu 3 tlačil na Holoubka.
    Plné znění variant: [`SS7_zaver_varianty.md`](SS7_zaver_varianty.md).

---

## 5. Tón a styl psaní

- Dokumenty a hlášení píšeme **byrokraticky, useknutě** — nikdy neřeknou
  víc, než musí. Odstavce co končí uprostřed myšlenky.
- Redigovaná jména = skutečně černé pruhy v assetu, ne kurzíva nebo
  "[REDACTED]" — vizuální, ne textová konvence.
- Rozhovory s pamětníky mají **odmlky a nedokončené věty** jako
  designový prvek, ne chybu — ticho nese informaci.
- Žádný jumpscare, žádná nadpřirozená hudba/zvuk. Napětí stojí na tichu
  a na tom, co lidi nechtějí říct.
- Historická kotva: Group A éra, Škoda 130 LR, Svazarm/AMK, jižní
  Morava u rakouské hranice, ÚSTR-styl archivní přístup. Realita kolem
  fiktivních jmen a fiktivní rallye.

---

## 6. Technická poznámka k integraci

- Nový `AppState` (např. `Archive`), vlastní scéna/UI — dokument
  viewer, dialogový systém pro rozhovory, případně jednoduchá "evidence
  board" obrazovka pro propojování zjištění.
- Auto se v tomhle módu používá jen jako **doprava mezi lokacemi** —
  žádný time trial, žádné tuning, klidná jízda. Může sdílet `World`/
  terrain trait z hlavní hry ve zjednodušené podobě.
- Nulová závislost na `SoftBody`, drivetrain, Pacejka modelu — striktně
  oddělený systém, nezasahuje do fáze 2 (Physics) roadmapy.
- Zapadá koncepčně do **fáze 5 (Game modes)** jako samostatný
  narrative/exploration mode vedle Freeroam a Rally checkpoints.
