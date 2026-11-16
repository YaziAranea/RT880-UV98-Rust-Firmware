# Project continuity

- Before each stage, read `FEATURES-NEXT-RU.txt`. It is the primary requirements and current-status registry. Historical `README`/`STAGE`/live-test documents describe their own snapshots; they do not override newer evidence in the registry.
- Add new user requirements under stable IDs with priority, acceptance criteria and current evidence. Preserve unresolved requirements and observations across context changes.
- At the end of a stage, update affected IDs, the installed-image/source snapshot and observation log. Distinguish code, host verification and observed hardware behavior; do not mark hardware acceptance from builds, simulated screens, ACKs or RF-state alone.
- Preserve original stock calibration, NOR backups, `999.880dmf` and immutable release artifacts. Edits use separate working copies. Firmware settings remain in RAM until an evidenced storage layout and write/recovery scheme are implemented.
- In the current multi-agent workflow, only the root agent runs Cargo or accesses COM/the radio. Other agents follow assigned file ownership and report evidence to root.
- Record the exact artifact/hash, verification report and limits of each result. An unobserved or failed acceptance criterion stays open.

- Public documentation and release descriptions must describe features, reproducible validation and limitations without assigning a release special status based on personal acceptance or private coordination.
