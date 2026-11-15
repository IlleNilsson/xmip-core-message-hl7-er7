# xmip-core-message-hl7-er7

HL7 v2 ER7 shape: segments as parts named by their tag, MSH-9 as the announced type; the one reader of the MSH, which the HL7 v2 contract reads a message's head by. A technology of [xmip-core-message](https://github.com/IlleNilsson/xmip-core-message).

The `MSH` is read once, here (`encoding`): MSH-1, the field separator, and
MSH-2's four encoding characters, each held to ASCII because the walk reads
bytes, none of the four the field separator. The HL7 v2 contract
([xmip-core-contract-hl7-v2](https://github.com/IlleNilsson/xmip-core-contract-hl7-v2))
reads a message's head by it, the one technology it depends on (ADR-0044,
amendment 2026-11-15), so the two read and refuse an `MSH` in the same
words.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
