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

## Findings

| Finding | Introduced By | Legacy Refs | Remediation Origin | Blocks | Category | Severity | Status | Disposition | Branch | Approved Target | Summary | Fixed By | History / Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F-0001 | CA-001 | — | — | — | Code;Test | P1 | Ready for Integration | — | `fix/F-0001-preserve-input` | 08e4ff527b873973365f2369045d0c20fa7c1f1d | 入力と同じ実体への出力で入力を切り詰める | 08e4ff52 | CR-002 passed at the exact branch target; same-file protection preserves input bytes for path and link aliases. |
| F-0002 | CA-001 | — | — | F-0006 | Code;Test | P1 | Ready for Integration | — | `fix/F-0002-F-0003-output-state` | 472d940fbc569b66e5425373fb15baf5a62ff781 | 保留レコードより後の結果を先に出しオフセット順を崩す | 472d940f | CR-001 passed at the exact target. This change is a prerequisite for F-0006. |
| F-0003 | CA-001 | — | — | F-0006 | Code;Test | P1 | Ready for Integration | — | `fix/F-0002-F-0003-output-state` | 472d940fbc569b66e5425373fb15baf5a62ff781 | 符号化ごとでない境界判定が文字列を欠落・分割させる | 472d940f | CR-001 passed at the exact target. This change is a prerequisite for F-0006. |
| F-0004 | CA-001 | — | — | — | Code;Test | P1 | Open | — | — | — | チャンク内だけの UTF-16LE 偶奇選別が正しい末尾を捨てる | — | チャンク幅 8 で「あいうえお」が「あいうえ」になる。 |
| F-0005 | CA-001 | — | — | — | Code;Test | P2 | Ready for Integration | — | `fix/F-0005-bound-inflight-chunks` | 3da034e5af0de1dd45bbdd24c8c3d5ea63a517e3 | 先行完了チャンクを無制限に保持し FD を使い切る | 3da034e5 | CR-003 passed at the exact branch target; the low-file-descriptor regression succeeds with one and four workers. |
| F-0006 | CA-001 | — | — | F-0004 | Code;Test | P2 | Open | — | — | — | 長い run の境界連結が入力全体をメモリへ蓄積する | — | 1 worker・1 MiB chunks でも 64 MiB の連続 ASCII に peak RSS 132.89 MiB。 |
| F-0007 | CA-001 | — | — | — | Code;Test | P2 | Ready for Integration | — | `fix/F-0007-odd-length-input` | 9097d0c8122134a268d5f00b80a6085541c658f7 | 入力長へのクランプで偶数のチャンク指定を誤って拒否する | 9097d0c8 | CR-004 passed at the exact branch target; odd-length input accepts valid even chunk widths. |
| F-0008 | CA-001 | — | — | — | Code;Test | P3 | Ready for Integration | — | `fix/F-0008-cancellation-report` | bb46f60f739b55148e8457a3b2b0efe7d15bec3b | SIGINT のチャネル切断をキャンセルより先に故障として報告する | bb46f60f | CR-005 passed at the exact branch target; cancelled channel shutdown is distinguished from unexpected worker loss. |

## Integration State

| Campaign | Integration Branch | State | Frozen Base | Last Integrated Target | History / Notes |
| --- | --- | --- | --- | --- | --- |
| CA-001 | — | Active | — | — | CR-001 approved F-0002 and F-0003; reviews for the remaining findings are pending. |

## Integrated Branches

| Campaign | Remediation Branch | Integrated Target | History / Notes |
| --- | --- | --- | --- |
