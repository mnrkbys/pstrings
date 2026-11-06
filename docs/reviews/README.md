# Review registry

`CA-001` の Campaign Branch は `review/audit-ca-001`。Integration Branch はまだない。初回の CA-001 report とこの registry の atomic publication commit が Campaign Anchor になる。監査対象のコードは `a299a58f52ea9a4b98f3ffa90ad68f4a7894d95e` であり、報告を追加したコミットとは区別する。

## Reviews

| Review | Type | Campaign | Reviewer | Previous | Base | Target | Date | Report |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CA-001 | Codebase Audit | CA-001 | Codex | — | 1f6b267e | a299a58f | 2026-10-23 | [CA-001.md](CA-001.md) |
| CR-001 | Code Review | CA-001 | Codex | — | 376d1de1 | 472d940f | 2026-11-06 | [CR-001.md](CR-001.md) |
| CR-002 | Code Review | CA-001 | Codex | CR-001 | 376d1de1 | 08e4ff52 | 2026-11-06 | [CR-002.md](CR-002.md) |
| CR-003 | Code Review | CA-001 | Codex | CR-002 | 376d1de1 | 3da034e5 | 2026-11-06 | [CR-003.md](CR-003.md) |
| CR-004 | Code Review | CA-001 | Codex | CR-003 | 376d1de1 | 9097d0c8 | 2026-11-06 | [CR-004.md](CR-004.md) |
| CR-005 | Code Review | CA-001 | Codex | CR-004 | 376d1de1 | bb46f60f | 2026-11-06 | [CR-005.md](CR-005.md) |
| CR-006 | Code Review | CA-001 | Codex | CR-005 | 472d940f | afd94f86 | 2026-11-06 | [CR-006.md](CR-006.md) |
| CR-007 | Code Review | CA-001 | Codex | CR-006 | afd94f86 | f2902479 | 2026-11-06 | [CR-007.md](CR-007.md) |
| CR-008 | Code Review | CA-001 | Codex | CR-007 | 79bcef84 | fbcf6160 | 2026-11-06 | [CR-008.md](CR-008.md) |

## Findings

| Finding | Introduced By | Legacy Refs | Remediation Origin | Blocks | Category | Severity | Status | Disposition | Branch | Approved Target | Summary | Fixed By | History / Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F-0001 | CA-001 | — | — | — | Code;Test | P1 | Resolved | — | `fix/F-0001-preserve-input` | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | 入力と同じ実体への出力で入力を切り詰める | 08e4ff52 | CR-008 verified path and link aliases remain rejected without truncating input in the integrated state. |
| F-0002 | CA-001 | — | — | F-0006 | Code;Test | P1 | Resolved | — | `fix/F-0002-F-0003-output-state` | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | 保留レコードより後の結果を先に出しオフセット順を崩す | 472d940f | CR-008 verified completed records retain global offset order with the full integrated output pipeline. |
| F-0003 | CA-001 | — | — | F-0006 | Code;Test | P1 | Resolved | — | `fix/F-0002-F-0003-output-state` | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | 符号化ごとでない境界判定が文字列を欠落・分割させる | 472d940f | CR-008 verified per-encoding continuation retains strings across the integrated scanner set. |
| F-0004 | CA-001 | — | — | — | Code;Test | P1 | Resolved | — | `fix/F-0004-global-utf16-parity` | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | チャンク内だけの UTF-16LE 偶奇選別が正しい末尾を捨てる | f2902479 | CR-008 verified parity candidates survive complete-run reassembly with integrated output spooling. |
| F-0005 | CA-001 | — | — | — | Code;Test | P2 | Resolved | — | `fix/F-0005-bound-inflight-chunks` | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | 先行完了チャンクを無制限に保持し FD を使い切る | 3da034e5 | CR-008 verified in-flight chunk files remain bounded with ordered output in the integrated pipeline. |
| F-0006 | CA-001 | — | — | F-0004 | Code;Test | P2 | Resolved | — | `fix/F-0006-bounded-records` | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | 長い run の境界連結が入力全体をメモリへ蓄積する | afd94f86 | CR-008 verified long-record spills preserve complete output across integrated ordering. |
| F-0007 | CA-001 | — | — | — | Code;Test | P2 | Resolved | — | `fix/F-0007-odd-length-input` | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | 入力長へのクランプで偶数のチャンク指定を誤って拒否する | 9097d0c8 | CR-008 verified explicit even widths accept odd-length UTF-16LE input in the integrated CLI. |
| F-0008 | CA-001 | — | — | — | Code;Test | P3 | Resolved | — | `fix/F-0008-cancellation-report` | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | SIGINT のチャネル切断をキャンセルより先に故障として報告する | bb46f60f | CR-008 verified cancellation remains distinct from unexpected channel loss in the integrated output stage. |

## Integration State

| Campaign | Integration Branch | State | Frozen Base | Last Integrated Target | History / Notes |
| --- | --- | --- | --- | --- | --- |
| CA-001 | CA-001-integration | Completed | 79bcef84640204189bb7d7161124d1c15baf004c | fbcf61604d1644a858f22da4c58dfb2cb0e74710 | CR-008 passed the complete integration range; all eight findings are resolved at the exact code target. |

## Integrated Branches

| Campaign | Remediation Branch | Integrated Target | History / Notes |
| --- | --- | --- | --- |
| CA-001 | fix/F-0001-preserve-input | 08e4ff527b873973365f2369045d0c20fa7c1f1d | Imported the exact independently approved branch Target. |
| CA-001 | fix/F-0002-F-0003-output-state | 472d940fbc569b66e5425373fb15baf5a62ff781 | Imported the exact independently approved branch Target. |
| CA-001 | fix/F-0004-global-utf16-parity | f29024795fc6335c644c1ac806970018b4306018 | Imported the exact independently approved branch Target. |
| CA-001 | fix/F-0005-bound-inflight-chunks | 3da034e5af0de1dd45bbdd24c8c3d5ea63a517e3 | Imported the exact independently approved branch Target. |
| CA-001 | fix/F-0006-bounded-records | afd94f86ff493f259a7de1c75d2f6600a6b5202d | Imported the exact independently approved branch Target. |
| CA-001 | fix/F-0007-odd-length-input | 9097d0c8122134a268d5f00b80a6085541c658f7 | Imported the exact independently approved branch Target. |
| CA-001 | fix/F-0008-cancellation-report | bb46f60f739b55148e8457a3b2b0efe7d15bec3b | Imported the exact independently approved branch Target. |
