## 024-effect-chain-rows-and-meters (contracts/fluent-strings.md): pt-BR
## values for this feature's 3 changed keys and 11 new keys (FR-015,
## NFR-7.1). Not yet a shipped locale (`tr`/`tr_args` still resolve against
## en-US only) — this bundle exists so the `fluent_keys` parity test proves
## every key here has a matching pt-BR translation, ready for when a locale
## switch ships.

## Changed values (same ids/placeholders as locales/en-US/effects.ftl).

effects-chain-cpu = CPU da cadeia: { $pct } % do orçamento em tempo real
effects-overloads = Excedentes de orçamento: { $count }
effects-cpu = { $pct } % do orçamento

## New keys.

effects-chain-cpu-hint = Parcela do tempo de cada callback de áudio gasta na cadeia de efeitos; um excedente é contado quando permanece acima de 90 % ou passa de 100 %.
effects-reorder-handle-node = Reordenar { $kind }, posição { $position }
effects-reorder-handle-hint = Arraste para reordenar, ou foque e pressione a seta para cima ou para baixo
effects-empty-explanation = Os nós de efeito processam o áudio no caminho até as caixas de som — alteram tom, tempo, timbre ou nível. Os nós são executados de cima para baixo, na ordem em que você os adiciona.
effects-empty-add = Adicionar nó de efeito
effects-spectrum-tick-100 = 100
effects-spectrum-tick-1k = 1k
effects-spectrum-tick-10k = 10k
effects-spectrum-ref-0db = 0 dB
effects-spectrum-ref-minus30 = −30
effects-spectrum-ref-minus60 = −60
