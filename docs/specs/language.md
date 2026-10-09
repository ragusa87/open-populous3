# Language files (original `language/langNN.dat`)

Every text the game shows (menus, debug options, level names, briefings, hints, messages), one file per
language. Parsed by `pop3_format::language` (`Language::load`, example `lang_info`). Read-only, never shipped.

## Layout [files]
- No header: texts follow each other, each ending with a 0 unit. A text's number is its position, from 0.
- Units are 16-bit little endian, but none is above 255: each holds one byte of a Windows code page, not UTF-16.
  Western languages use 1252 (0x85 is an ellipsis, 0x99 a trademark sign), Polish 1250 (0xB3 is ł there, ³ read
  as Latin-1). The parser turns them into Rust strings (UTF-8).
- Every full file has the same 1 318 texts, so a number is the same text in every language.

| File | Language | Bytes |
|---|---|---|
| lang00 | English | 207 264 |
| lang01 | French | 235 608 |
| lang02 | German | 247 476 |
| lang04 | Spanish | 240 972 |
| lang05 | Swedish | 209 428 |
| lang06 | Dutch | 243 402 |
| lang07 | Polish | 229 432 |
| lang03, lang08-11 | placeholders: `20 0D 0A` (3 bytes), no text | 3 |

(French release checked here.)

## Contents [files]
This spec never quotes the original texts: it gives their numbers and what kind of text they are.
- From 0: key names, error messages, debug options.
- The campaign texts are grouped per level: its name, then its briefing, then its hints and messages, in level
  order. Level 3's name is text 647 and its briefing 648; level 4's name is 653, level 5's 660.
- The last text is an error message.

## Open
- Which text number names each level: the groups have different lengths, and no table in `POPTB.EXE` pairs 647
  with 653 (u16 or u32). Maybe the scripts' message numbers (`CREATE_MSG_NARRATIVE idx`, ai-scripts.md) are text
  numbers with an offset, or the level header holds one: to find.
