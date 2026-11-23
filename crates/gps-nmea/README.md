# GPS NMEA foundation

Pure `no_std`, allocation-free parser. Not connected to hardware, firmware,
display or CPS. No claim that a GPS module is present or works on this radio.

`Parser::push(byte, now_ms)` recognizes GP/GN/BD GGA and RMC only after a valid
XOR checksum and complete CRLF. The buffer holds at most128 bytes between `$`
and CRLF. New `$` resynchronizes; overflow and malformed lines discard only the
partial input. Unsupported sentences do not refresh a fix.

`snapshot(now_ms, max_age_ms)` returns only a fresh receiver-valid position.
`last_report()` is diagnostic and can be stale or invalid. Explicit GGA quality0,
RMC statusV or modeN removes position immediately. Other quality/mode values are
retained: callers must distinguish estimated/manual/simulated positions from
satellite solutions. There is no cross-sentence metadata merge. Dates retain the
two-digit year; no century or date for GGA is inferred. Coordinates round to e7;
HDOP rounds to hundredths and is not an accuracy measurement.

`motion_snapshot(now_ms, max_age_ms)` returns the position, optional speed in
hundredths of a knot and optional true course in hundredths of a degree from
one fresh RMC sentence. Valid GGA does not change this sample. A no-fix report
or a new RMC marked estimated, manual or simulated clears it. Course 360° maps
to 0°. Time and date fields may be absent.

Time uses wrapping u32 milliseconds and intervals below2^31. Backwards sentence
timestamps are rejected. Choose an expiry appropriate to the actual update rate.

Protocol reference: [u-blox7 Receiver Description / Protocol Specification,
sections20–24](https://content.u-blox.com/sites/default/files/products/documents/u-blox7-V14_ReceiverDescriptionProtocolSpec_%28GPS.G7-SW-12001%29_Public.pdf).
This reference documents NMEA fields; it does not identify this radio's module.
The classic fixed-checksum GGA/RMC fixtures are supplemented by malformed,
no-fix, coordinate-boundary, framing, resynchronization and exhaustive byte tests.

`stock_baud()` reflects CFG+0xD7 and all256 selectors checked in local stock
evidence `research/radio018/aux-evidence.json`: indices0..9 map to
4800,9600,14400,19200,38400,56000,57600,115200,128000,256000; others9600.
It does not access IO. The stock UART collector uses a100-byte buffer and ends
at `*HH`; this parser deliberately requires CRLF and is therefore stricter.
