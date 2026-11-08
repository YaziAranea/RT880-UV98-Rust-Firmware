UV98 UART observer0.1.2 — приложение для одного раннего наблюдения

Этот crate не заменяет внутренний загрузчик. Область приложения остаётся
0x08002800..0x0803FFFF. Векторы повторяют два первых слова работавшего стока:
MSP0x200099C8 и Reset0x08002AC1. Таблица занимает704 байта,176 слов.
Это устранение двух различий формата, не доказательство требований загрузчика.

Rust обеспечивает сборку и global_asm. Из Reset не вызывается Rust. Нет
инициализации .data/.bss, ручных обращений к RAM, BL, push/pop или heap.
MSP задаётся для возможного аппаратного exception stacking; сам stacking,
пригодность SRAM и nested faults требуют проверки на железе.
Reset не возвращается. LR и r4-r11 не обязаны сохраняться для загрузчика.

В отличие от0.1.1 семействоMCU, PID и revision НЕ используются как условие
разрешения UART. Используется явное допущение: карта CRM/GPIO/USART совпадает
с операциями стокового1.16, уже успешно работавшего на данной станции.
Первый постоянный кадр передаётся до чтения CPUID/PID/SER_ID/Flash-size.
IPSR и CONTROL сохраняются как наблюдения, не как молчаливые отказы входа.

Порядок
1. СохранитьCONTROL/IPSR/entryMSP вCPU регистрах, маскировать IRQ, задать
   stockMSP и VTOR приложения. Унаследованный r12 готовности UART сбрасывается
   ещё до любогоMMIO. Сохранить исходныеCTRL/CFG/MISC1/MISC2/PLLCFG.
2. ОстановитьSysTick, очистить восемьNVIC ICER/ICPR banks иpendingSysTick/PendSV,
   снятьSLEEPONEXIT/SLEEPDEEP, остановить/reset DMA1/2 иOTG передPA11.
3. Установить HICK8MHz: auto-step, включитьHICK/дождатьсяready; direct/16,
   40NOP, очиститьHICK_TO_SCLK(bit14), переключитьSCLK вHICK/дождатьсяstatus;
   AHB/APB2 /1, directHICK /1,40NOP,auto-stepoff; проверить итоговые поля.
   HICKDIV(bit12), PLLCFG, PLL/HEXT enable и APB1div сохраняются.
   FLASH/PWC/RTC/WDT не изменяются. Точная последовательность с основаниями:
   research/uart-probe/CLOCK-BOOTSTRAP-RU.txt.
4. USART6 TX-only1152008N1, divisor69 при номинальных8MHz; PA11AF8,
   push-pull, STRONGER drive1 как в стоке, безpull. PA12 и остальныеGPIO
   поля сохраняются. Нет приёмника команд, IRQ/DMA/flow control UART.
5. Передать13 полных кадров, затемWFI. Обработка исключений — один faultкадр,
   только еслиUART успел настроиться; без повторнойпроверкиID/clockbootstrap.

Формат UV98D1 такой же, как в0.1.1:47 ASCIIbytes,
UV98D1 CCCCCCCC AAAAAAAA BBBBBBBB DDDDDDDD*SS\r\n
SS=sum8 первых42bytes до*. Все числаuppercaseHEX.
События:
  200 [00000102,0,0] — постоянный маркер настроенногоUART;
  201 [CONTROL,IPSR,entryMSP];
  202 [initialCRM_CTRL,initialCRM_CFG,0];
  203 [initialCRM_MISC1,initialCRM_MISC2,initialCRM_PLLCFG];
  210 [E000ED00,0,0] — передCPUID;211 [value,0,0] — после;
  212 [E0042000,0,0] — передPID;213 [value,0,0] — после;
  214 [E0042020,0,0] — передSER_ID;215 [value,0,0] — после;
  216 [1FFFF7E0,0,0] — передFlash-size halfword;217 [value,0,0] — после;
  220 [0,0,0] — все наблюдения закончены, дальшеWFI;
  E2FF0000 [IPSR,CFSR,HFSR] — terminal exception после настройкиUART.
Каждый ID читается ровно один раз. Кадр перед чтением полностьюdrainsTDC.

Границы
Нет обращения кRF-чипам, LCD, NOR или калибровкам. Ни одна RF control GPIO
не конфигурируется: наблюдатель сохраняет их унаследованное состояние и
не доказывает электрическоеRF-off. Экран намеренно не включается; чёрный
экран сам по себе не является критерием провала этого конкретного образа.
Критерий — байты и этапыUART. Здесь не реализована работающая рация.

Ожидания clocks ограничены100000 чтений каждое; TDBE/TDC —8192 чтения.
Любой timeout ведёт вWFI без повторов. Watchdog не кормится и может вызвать
reset/обрыв кадра. ЗаписьFlash/NOR и восстановлениестока отсюда невозможны.
Ошибки до готовности UART всё ещё могут остаться невидимыми: MCU/clock map,
отсутствиеHICK, невозможностьclockswitch, GPIOlock, неполадкикабеля, ранний
fault/stacking. LockedPA11 не обходится: записьконфигурации может игнорироваться
железом. Это не гарантия вывода на любой плате/MCU или при любомhandoff.

Build/audit/экспорт и liveflash выполняются отдельно главным процессом.
Этот crate не открываетCOM и не содержит host-прошивальщика.
