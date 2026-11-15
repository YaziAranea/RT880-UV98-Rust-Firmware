# Contributing

Issues and contributions are welcome in English, German or Russian. Start with [BUILDING.md](BUILDING.md) for public build commands and the fixture-free test subset. Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md) for a reproducible problem.

## Choose a focused change

Read the current release notes, [HANDOFF.md](HANDOFF-R36.md) and [FEATURES-NEXT-RU.txt](FEATURES-NEXT-RU.txt) before changing established behaviour. The requirements registry contains historical observations as well as current work; preserve unresolved user requirements. Keep unrelated refactoring out of a feature or bug fix.

For a pull request, describe the concrete problem, resulting behaviour and checks actually performed. Include a small before/after example or screen when useful. Distinguish host tests, simulated output and a result observed on a named radio/build. A successful compile, loader acknowledgement or changing RSSI does not establish working audio or transmission.

## Preserve radio data

Keep original calibration, NOR backups and stock codeplugs unchanged. Work on copies and new output directories. Do not add personal dumps or fixtures to Git, issues or release archives; follow [PRIVATE-INPUTS.md](PRIVATE-INPUTS.md).

Do not bypass calibration, image or catalog validation to make a build appear compatible. A new board revision requires evidence for that hardware. Settings currently remain in RAM; persistent storage changes need an established layout and recovery procedure.

## Code, language and visuals

Run the checks appropriate to your change, using the public subset where private golden inputs are unavailable. Do not fabricate missing golden fixtures. Firmware and simulator share the renderer: keep their output consistent. Label simulated screenshots as simulations and do not present generated signal plots as RF measurements.

For UI text, update the English, German and Russian translations together, preserve format placeholders and check the result at the real display size. Keep technical abbreviations recognizable and channel names unchanged. Update the corresponding user guide when controls or CPS steps change.

Original contributions use the project's [MIT licence](LICENSE). Preserve third-party licences and attribution described in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md); do not copy manufacturer firmware or another project's code into a release without checking its terms.
