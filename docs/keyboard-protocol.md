# K616 protocol notes

Verified on this keyboard.

## Transport

Four 1032-byte HID feature reports (report id 6) per change. The 6-byte
report-5 handshake in the K617 captures is rejected by Windows and unnecessary.

## Block 4 layout

```
offset  0..8    header: 06 03 b6 00 00 00 00 00
offset  16..80  64-byte effect payload
    +0x00..0x14  mode-specific
    +0x14..0x16  FF FF
    +0x16..0x3E  twenty 2-byte pairs: [0x07, speed<<4 | brightness]
    +0x3E..0x40  5A A5 (terminator)
offset  21       mode id
```

The vendor's `KeyboardDrv.exe` builds this payload (0x4110CA writes the
`5A A5`); it fills **all twenty** pairs, which is why patching only two of them
changed speed but not brightness.

## Mode ids

The vendor list order is the firmware id:

| Name | Id | Name | Id |
|---|---|---|---|
| Off | 0x00 | Neon stream | 0x0b |
| Fixed on | 0x01 | Reaction | 0x0c |
| Respire | 0x02 | Sine wave | 0x0d |
| Rainbow | 0x03 | Retinue scanning | 0x0e |
| Flash away | 0x04 | Rotating windmill | 0x0f |
| Raindrops | 0x05 | Colorful waterfall | 0x10 |
| Rainbow wheel | 0x06 | Blossoming | 0x11 |
| Ripples shining | 0x07 | Rotating storm | 0x12 |
| Stars twinkle | 0x08 | Collision | 0x13 |
| Shadow disappear | 0x09 | Perfect | 0x14 |
| Retro snake | 0x0a | Game mode | 0x15 |

Eight of these were confirmed against K617 captures; the rest follow from the
id mapping and are applied with the same template.

## If a mode misbehaves

Some modes may want parameters beyond colour/speed/brightness. A USB capture
(USBPcap + Wireshark) of the vendor app setting that mode gives the exact
64-byte payload; diffing it against the template shows which bytes to add.
