# Bundled fonts

| File | Font | Licence |
|---|---|---|
| `Ubuntu-Light.ttf` | Ubuntu Light (from egui) | Ubuntu Font Licence 1.0 (`UFL.txt`) |
| `Hack-Regular.ttf` | Hack (from egui) | MIT / Bitstream Vera (`Hack-Regular.txt`) |
| `emoji-icon-font.ttf` | emoji-icon-font (from egui) | MIT (`emoji-icon-font-mit-license.txt`) |
| `NotoEmoji-Regular.ttf` | Noto Emoji, © 2013 Google LLC | SIL Open Font Licence 1.1 (`NotoEmoji-OFL.txt`) |

`NotoEmoji-Regular.ttf` is a modified version of `NotoEmoji[wght].ttf` from
<https://github.com/google/fonts/tree/main/ofl/notoemoji>, reduced with fontTools to a static Regular
instance without layout tables or hinting, because egui draws one character at a time:

```sh
fonttools varLib.instancer 'NotoEmoji[wght].ttf' wght=400 -o static.ttf
pyftsubset static.ttf --unicodes='*' --layout-features='' --no-hinting --desubroutinize \
    --drop-tables+=GSUB,GPOS,GDEF,STAT --name-IDs='*' --output-file=NotoEmoji-Regular.ttf
```
