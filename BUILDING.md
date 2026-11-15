# Сборка R35

R35 build3 от 16 ноября 2026 года собрана от принятой R33 с профилем channel-tx. Это проект исследованной платы AT32F423 и калибровок владельца, а не универсальный установщик.

## Зависимости и входы

Проверенная конфигурация: Rust 1.99.1, host x86_64-pc-windows-gnullvm, target thumbv7em-none-eabi. Файла rust-toolchain.toml нет; Cargo.lock фиксирует зависимости, но не компилятор.

Нужны PowerShell 7, Rust/Cargo и Python 3.11+ с capstone, unicorn, pyelftools. Инструменты кабельного журнала отдельно используют pyserial. Нужны личные входы по [PRIVATE-INPUTS.md](PRIVATE-INPUTS.md):

- Fixture в crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin.
- Исходный NOR 4 МиБ, передаваемый параметром -NorBackup.

Скрипт проверяет SHA до и после работы. Данные другой станции требуют отдельной валидации, а не отключения проверок. Обычная сборка проверяет сохранённые oracle; их повторная генерация по reference BIN — отдельная операция.

## Подготовка

Из корня рабочего проекта:

~~~powershell
$env:PATH = (Join-Path $env:USERPROFILE '.cargo/bin') + ';' + $env:PATH
rustup target add thumbv7em-none-eabi
cargo fetch --locked
python -m pip install capstone unicorn pyelftools pyserial
~~~

Сборочный сценарий использует --offline после загрузки зависимостей. На Windows gnullvm выбирается rust-lld из установленного sysroot; CPS дополнительно запускает tools/prepare_windows_link.py.

## Прошивка

~~~powershell
./build-diagnostic-image.ps1 -ChannelTx -NorBackup 'D:/private-radio-backup/storage-address-space.bin' -OutputDirectory 'D:/radio-builds/uv98-next-build'
~~~

Заменить пути реальными локальными входами. Выходной каталог обязан быть новым. Сценарий выполняет форматирование, oracle, host-тесты, ARM ELF/BIN, проверки раннего запуска и симуляцию на NOR. COM не открывается, рация не прошивается.

Без -ChannelTx получается RX-профиль. Исторический -UpperExoTxTrial не является профилем R35; два TX-профиля одновременно запрещены.

Результаты: verification.json, build-log.txt, image/ и preview/. Внутреннее имя экспортера сохраняется:

~~~text
image/UV98-diag-0.1.8.bin
image/UV98-diag-0.1.8.elf
image/diagnostic.map
~~~

При выпуске проверенный BIN копируется под именем UV98-r35.bin. Версию определять по SHA, не только имени. Проверенная сборка находится в outputs/UV98-r35-20261116-build3 исходного workspace.

## CPS и упаковка

~~~powershell
./build-cps.ps1
python tools/package_diagnostic_image.py D:/radio-builds/uv98-next-build --profile channel-tx --exclude-private-fixture
~~~

CPS появляется в target/cps-portable/, отчёт — validation/cps-build-summary.json. При необходимости копируется libunwind.dll. Сборка не заменяет проверку нативного интерфейса.

Без --exclude-private-fixture упаковщик включает личный fixture в source.zip. Такой архив нельзя загружать в GitHub. Опция исключает fixture из архива, но не отменяет его необходимость для сборки. Перед передачей проверить также отсутствие NOR, codeplug и личных журналов.

Комплект R35 содержит очищенные исходники, CPS, R35, R33 и заводской recovery BIN 1.16. Упаковка ничего не записывает на станцию.

## Зафиксированный результат build3

~~~text
BIN: 251904 байта
SHA256: 97e3d98257cacef6d64917d4b355719e1649b8570e8df8e9c9527edd39c7307d
Flash: 251400 байт занято, 504 байта свободно
RAM над статическими данными: 38828 байт
~~~

RAM — расчёт ELF, а не измеренный пик стека. Прошли 547 Rust-тестов, 122 CPS-теста, 80 Python-тестов; один Rust-тест пропущен. Выполнены 59 ARM-проверок, 59 проверок декодирования и 83 PNG-сценария.

Это результаты на ПК, не аппаратная приёмка. В многопоточном процессе Cargo и COM использует только корневой агент. Старые COM20/COM21 инструкции и tools/session-archive/ относятся к прошлым сессиям; актуальный порт определяется отдельно.

