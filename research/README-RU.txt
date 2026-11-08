Доказательства и воспроизведение машинных эталонов

frequency/, general/, rf/ содержат выбранные фрагменты дизассемблирования,
описания проверенных путей, генераторы и manifests. Rust-тесты используют
готовые CSV. Обычная build.ps1 не требует Unicorn или исходного stock BIN.
tools/check_oracles.py сверяет SHA256 и количество строк с этими manifests.

Для повторной генерации нужен Python и Unicorn 2.1.4:

  python -m pip install --target research/deps -r research/requirements-oracle.txt

Запускать из корня этого Rust-проекта. В PowerShell задать:

  $env:UV98_RUST_PROJECT = (Get-Location).Path
  $env:UV98_STOCK_REFERENCE = 'полный путь к исследованному RT880 1.16 BIN'
  python research/frequency/derive_frequency.py --skip-disassembly
  python research/general/oracle.py
  python research/rf/oracle_rf.py
  python tools/check_oracles.py

Допускается только SHA256
eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc.
stock_emu.py отклоняет другой образ. Ссылка на источник — в CALIBRATION-PORT-RU.txt.
BIN не включён в архив и не предлагается как образ восстановления UV98.
Проверки создают только локальные CSV/JSON. COM, сеть и аппаратные
периферийные адреса не используются. RF/SPI операции заменены перехватами.

Генерация дизассемблирования отдельно требует Capstone 5.0.7; сохранённых
фрагментов достаточно для чтения evidence. Рабочие analyze/triage scripts
содержат ссылки на локальные исходные файлы исследования; это не приложение CPS.
cps/triage.json фиксирует SHA и сравнение методов двух изученных CPS.
Полная декомпиляция EXE, исходные EXE и сторонние двоичные библиотеки в архив
не включены. Исходная программа CPS не запускалась.
