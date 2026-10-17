# Offene Fragen aus dem Schreiben der Testfälle

Stellen, an denen SPEC.md, das Referenzprofil oder tests/README.md eine Frage
nicht eindeutig beantworten. Zu keiner davon gibt es einen Testfall. Die
Reihenfolge entspricht ungefähr der Zahl der Fälle, die an der Antwort hängen.

## A · Blockiert ganze Bereiche

1. **Kanonische Form von *lead*-Inhalt** (C6). `<li>a<ul>…</ul></li>`: steht
   `a` direkt hinter `<li>`, oder kommt `a` wie im konsona-2-Korpus auf eine
   eigene eingerückte Zeile? „Line breaks only between blocks“ und „phrasing
   on exactly one line“ lassen beides zu. Daran hängen alle verschachtelten
   Listen, `li` mit Absatz danach, die Listentiefe (`limit-exceeded`) und
   Callouts in Listen (`constraint-violated`).
2. **Leerzeilen in eingerückten Kontexten.** Ist die Leerzeile vor einer
   Überschrift in einem Container (C9) leer, oder trägt sie die zwei
   Leerzeichen der Ebene? Dieselbe Frage stellt sich in der Notation: Steht
   unter einer Elementzeile mit zwei Blöcken (`.div` mit zwei Absätzen,
   `.figure`, `.td` mit `a`-Inhalt) eine leere Zeile oder eine mit
   Einrückung? Daran hängen alle Container mit mehr als einem Block.
3. **Leerzeilen zwischen `.li`-Zeilen** in der Elementzeilen-Form einer Liste
   (§5.6, leeres Item, zwei aufeinanderfolgende Listen). Gilt dort „one blank
   line between two blocks“? Für die Kurzform zeigt das Beispiel in §7.3:
   keine Leerzeile.
4. **`<pre>` in einem Container.** C8 sagt „not indented“. Gilt das nur für
   den Inhalt, oder wird auch der Start-Tag nicht eingerückt? Daran hängen
   Code in Blockquotes, Callouts und Containern.
5. **`figure` mit direktem `img`.** Laut Profil erlaubt `figure` nur
   *blocks*, `img` ist inline. Damit ist `<figure><img …><figcaption>` nicht
   im Profil (im konsona-2-Korpus kommt es aber vor). Soll `img` direkt in
   `figure` stehen dürfen?

## B · Testformat (tests/README.md)

6. **Kind `diagnostic`:** Welche Eingabedatei gilt (`input.txt` oder
   `input.html`)? Ist `expected.html` dann trotzdem Pflicht?
7. **Kind `passthrough`:** `expected.html` heißt „required“. Ist es eine Kopie
   von `input.html`? (So habe ich es gemacht.) Und welche Operation muss die
   Bytes zurückgeben: `canon`, Lesen und Schreiben, oder beides?
8. **BOM, CRLF, CR, fehlendes oder doppeltes Schluss-LF** (C1, C2) sind nicht
   testbar: Die Konvention verlangt UTF-8 ohne BOM, LF und genau ein
   Schluss-LF für jede Datei. Dasselbe gilt für Notation, die mit Leerzeilen
   endet. Gebraucht wird ein Ausweg, etwa eine Rohdatei, deren Bytes
   ungekürzt gelten.
9. **Fehlt `diagnostics.txt`,** heißt das dann „keine Diagnose erwartet“
   oder „nicht geprüft“?
10. **`.editorconfig`** setzt `trim_trailing_whitespace = true` für alle
    Dateien außer `*.md`. Ein Editor entfernt damit die Leerzeichen am
    Zeilenende, auf die es in einem Testfall ankommen kann. Vorschlag:
    `tests/.editorconfig` mit `trim_trailing_whitespace = false`.

## C · Kanonische Serialisierung

11. **`pre` und NFC:** Wird `pre`-Inhalt nach NFC normalisiert (C3), oder ist
    er unberührt (C8 „verbatim“)? Und gilt C4 (`&nbsp;` usw.) in `pre`?
12. **C4 in Attributwerten:** Wird ein U+00A0 in `alt` als `&nbsp;`
    geschrieben? C5 nennt für Attribute nur `&` und `"`.
13. **Klassen:** Werden Leerzeichen am Anfang und Ende entfernt, doppelte
    Klassen zusammengefasst, und bleibt ein leeres `class=""` stehen?
14. **C7 und Inline-Grenzen:** Reicht „removed at the start and end of a
    block“ in ein Inline-Element hinein (`<p><strong> a</strong></p>`)? Und
    gilt „next to a `<br>`“ auch, wenn das Leerzeichen innerhalb eines
    Inline-Elements neben dem `<br>` steht (`<em>a </em><br>`)?
15. **`start="1"` in HTML-Eingabe:** Entfernt `canon` das Attribut, oder ist es
    `out-of-profile`? Das Profil sagt nur „omitted when 1“.
16. **Kommentare:** Wo steht ein Kommentar zwischen Blöcken in kanonischem
    HTML (eigene Zeile, eingerückt)? Und wie zeigt die Notation ihn, mit
    Leerzeilen davor und danach oder ohne? (Getestet ist bisher nur das
    Durchreichen.)

## D · Profil und Durchreichen

17. **`h5`/`h6` in HTML:** Laut Anhang A wird tiefere Eingabe „reduced with a
    diagnostic“. Nach §3.3 wird sie als `out-of-profile` durchgereicht. Was
    gilt für HTML, was für Notation? (Notation: §6, getestet.)
18. **Verletzte Constraints und Limits in HTML-Eingabe** (Text direkt im
    `div`, drei Containerebenen, `summary` nicht als erstes Kind, Callout im
    `li`): Welche Codes kommen, `out-of-profile`, `constraint-violated`,
    `limit-exceeded` oder mehrere? Und wird der Block durchgereicht?
19. **Bäume aus Notation, die Limits überschreiten** (§3.4): Wie wird der
    Baum gespeichert, kanonisch formatiert oder anders? Und was steht im
    Rohblock, wenn es noch keine „stored bytes“ gibt?
20. **`pre` mit Elementen:** §5.11 nennt „a pre holding elements as read from
    a foreign file“ einen Block *innerhalb* des Profils. C8 und das Profil
    sagen, `pre` enthält nur Text.
21. **Inseln:** Welches `<name>` in `data-<name>-rendered` legt die
    Testsuite zugrunde? Sie soll ja implementierungsneutral sein. Welcher
    Diagnosecode gilt, wenn eine Insel ihre Whitelist verletzt (Skript,
    `onload`, `foreignObject`, `javascript:`, `@import`)? Und sind
    `data-<anderer>-*`-Attribute einer fremden Implementierung
    `out-of-profile`?
22. **„`src` never absolute“:** Zählt ein wurzelrelativer Pfad (`/bild.png`)
    dazu?

## E · Notation

23. **W2 bei Listennummern:** `<p>1. Mai</p>`. W2 sagt: Backslash vor das
    *erste* Zeichen des Konstrukts. `\1` ist aber literal, weil `1` kein
    Satzzeichen ist. Ist die Anzeige `1\. Mai`?
24. **W2 bei mehreren Konstrukten:** Wie weit wird escapt? Beispiel
    `<p>**a** und **b**</p>`: `\**a**` liest sich als `*` + `<em>a</em>` +
    `*`. Und was ist die Anzeige von `<em>*a</em>`, dem eigenen Beispiel aus
    §5.5.2 (`**a*`)? Reicht es, so wenig wie möglich zu escapen, oder muss
    jedes erkennbare Konstrukt escapt werden?
25. **Getipptes HTML, das nicht übernommen wird** (`<blink>`, ein
    ungeschlossenes `<strong>`): Escapt der Writer das `<`? Und wird
    `html-not-taken` beim erneuten Lesen der Anzeige wieder gemeldet?
26. **Elementzeile, Randfälle:** Groß- und Kleinschreibung (`.P`), ein
    Attribut außerhalb des Profils in der Klammer (`.p[style="…"]`), Inhalt
    an einem leeren Element (`.hr(x)`), `id` in der Inline-Langform
    (`.span#x(a)`): Ist das Text, ein Fehler oder `out-of-profile`?
27. **Mehrere eingerückte Phrasing-Zeilen** unter `.p.lead`: Ergibt das
    `<br>` wie in §5.2 Regel 1? Naheliegend, steht aber nicht da.
28. **Eingerückte Textzeile ohne Elementzeile darüber** (`  Text` auf oberster
    Ebene): Ist das Text oder ein Fehler?
29. **Reihenfolge der Attribute in `[…]`** bei der Anzeige: C11 oder die
    Reihenfolge der Eingabe? (In den Fällen ist sie bisher ohnehin
    alphabetisch.)
30. **Mehrere Klassen an `pre`:** Wie stehen sie in der Info-Zeichenkette
    des Zauns?
31. **Zaun und Zeilenenden:** Wie wird `<pre>x\n</pre>` (Inhalt endet mit
    LF) angezeigt? Und ist ```` ```⏎``` ```` oder ```` ```⏎⏎``` ```` die
    Anzeige eines leeren `pre`?
32. **Nicht geschlossene Konstrukte:** Was passiert bei einem Zaun, einem
    `:::`-Container oder einem getippten HTML-Block ohne Abschluss, und was
    bei `<!--` ohne `-->` (Baum und `unclosed-comment`)? Und ist ein
    Backtick ohne Gegenstück literal (naheliegend nach CommonMark)?
33. **`:::warnung`**, wenn der Name kein Element ist: Pandoc liest ihn als
    Klasse, §6 als Element. Ist das Text, ein `div.warnung` oder ein Fehler?
34. **Attributklammer mit nacktem Wort** (`{wichtig}`): „not accepted“, aber
    wird die Zeile dann Text, und gibt es eine Diagnose?
35. **`attribute-conflict`:** Welcher Wert gewinnt, und wie sieht der Baum
    aus?
36. **`>a` ohne Leerzeichen:** Wird das als Blockquote gelesen?
37. **Tabellen:**
    - Zählt das `= ` einer Kopfzelle zur Spaltenbreite? (In den Fällen
      haben alle Zellen einer Spalte das Präfix oder keine.)
    - Welche Trennzeilen sind gültig (`|-|-|`, `|:---|---:|`)?
    - Ein Raster mit uneinheitlich vielen Zellen pro Zeile als Eingabe:
      Welcher Baum entsteht?
    - Ein getipptes Raster mit mehr als sechs Spalten: angenommen, oder
      `limit-exceeded`?
    - Die `a|`-Zelle: Wie weit wird eingerückt, und darf nach `a|` noch
      etwas auf der Zeile stehen?
    - Ein zweizeiliger Absatz direkt über dem Raster: Absatz und Tabelle,
      oder etwas anderes?
38. **`callout-weight-mismatch`:** Ich habe es so verstanden, dass der Code
    bei jedem Lesen eines nicht passenden Callouts kommt
    (`callouts/weight-mismatch`). §7.4 erwähnt ihn nur für einen
    *geänderten* Registereintrag. Bitte bestätigen.

## F · Eindeutig, aber vermutlich nicht gewollt (Fälle sind geschrieben)

- `serialization/href-query-encoded`: Nach C12 wird in einem relativen Pfad
  `?` zu `%3F` und `=` zu `%3D`. Aus `suche.html?q=eisen` wird so
  `suche.html%3Fq%3Deisen`, und der Link geht kaputt.
- `serialization/href-second-hash-encoded`: Ein zweites `#` im Fragment wird
  zu `%23`. Das ist harmlos, aber vermutlich nicht beabsichtigt.
- §5.9 zeigt die Cell-per-line-Form an einer Tabelle, die der Writer als
  Raster anzeigen würde. Das ist kein Widerspruch, kann einen Leser aber in
  die Irre führen. `tables/cell-per-line-input` legt das fest.
