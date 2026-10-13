# ModPlayerKeys-Regular.ttf

Three-glyph subset of **Noto Sans Symbols 2** used to draw the macOS modifier
symbols in key labels: U+2318 `⌘`, U+21E7 `⇧`, U+2325 `⌥`.

- Upstream: https://github.com/notofonts/noto-fonts (`unhinted/ttf/NotoSansSymbols2/NotoSansSymbols2-Regular.ttf`)
- Version: 2.003
- Licence: SIL Open Font License 1.1 (see `OFL.txt`, from https://github.com/notofonts/symbols)
- Subset command:

  ```sh
  pyftsubset NotoSansSymbols2-Regular.ttf \
    --unicodes=U+2318,U+21E7,U+2325 \
    --output-file=ModPlayerKeys-Regular.ttf
  ```

  (`python3 -m fontTools.subset …` is equivalent; on Debian install
  `python3-fonttools`.)

- Fallback: if `pyftsubset` is unavailable, ship the full
  `NotoSansSymbols2-Regular.ttf` under the same file name and note it here.
  Only the three glyphs above are required.
