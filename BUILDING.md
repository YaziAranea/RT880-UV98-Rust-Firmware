# Сборка снимка разработки

Исходники сохранены из рабочего проекта r5. Это пока профиль конкретной
UV‑98 Plus/родственной платы RT880 с AT32F423, а не универсальный установщик.

## Входы

- Rust/Cargo с зависимостями из `Cargo.lock`.
- Цель `thumbv7em-none-eabi`.
- Python 3.11 или новее (нужен стандартный `tomllib`) с зависимостями исследовательских проверок (`capstone`, `unicorn`,
  `pyelftools`; `pyserial` нужен для кабельного журнала).
- PowerShell 7 для имеющихся сборочных сценариев.
- Приватные fixture/NOR по [PRIVATE-INPUTS.md](PRIVATE-INPUTS.md).

Сначала вернуть свой сохранённый calibration fixture в игнорируемый Git путь:

```text
crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin
```

Для полного сценария нужен также исходный 4 MiB NOR. Проверки SHA в скрипте
намеренно соответствуют сохранённым входам этого проекта; перенос на другую
станцию требует отдельного процесса валидации её калибровок.

## Команды

```powershell
rustup target add thumbv7em-none-eabi
cargo fetch --locked
./build-diagnostic-image.ps1 -ChannelTx `
  -NorBackup 'D:/private-radio-backup/storage-address-space.bin' `
  -OutputDirectory 'D:/radio-builds/new-build'
```

Выходной каталог должен быть новым. Сценарий проверяет форматирование, oracle,
host-тесты, ARM ELF/BIN, ранний запуск и симуляцию. Он **не прошивает рацию**.
Без `-ChannelTx` получается RX-профиль. Исторический `-UpperExoTxTrial` сохранён;
два TX-профиля одновременно запрещены.

Сборка CPS:

```powershell
./build-cps.ps1
```

Перед запуском проверить параметры сценария и платформенные зависимости в
`README-RU.txt`, `CPS-PLAN-RU.txt` и `apps/cps/`.

## Упаковка

```powershell
python tools/package_diagnostic_image.py D:/radio-builds/new-build --profile channel-tx
```

Локальный `source.zip` включает calibration fixture, необходимый тестам.
Поэтому такой полный release ZIP нельзя переносить в будущий публичный репозиторий
без подготовки отдельного пакета исходников без персональных входов.

`tools/boot-log/` — ограниченный RX-only приёмник журнала. Он привязан к COM20/CH340
для данной сессии. `tools/session-archive/` содержит точные рабочие инструменты
прошивания как архив: их пути и SHA привязаны к исходному workspace. Они намеренно
не превращены в универсальный прошивальщик при подготовке документации.

Портативный установщик, автоматическое определение платы и CI без персональных
fixture остаются отдельными задачами. README не объявляет их выполненными.
