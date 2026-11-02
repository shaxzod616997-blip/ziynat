<p align="center">
  <img src="assets/app-icon/hicolor/256x256/apps/ai.storyteller.photocraft.png" width="128" height="128" alt="ZIYNAT">
</p>

<h1 align="center">ZIYNAT</h1>

<p align="center"><b>Rasm muharriri — qatlamlar, niqoblar, filtrlar, matn, vektor va PSD fayllar.<br>
Internetsiz ishlaydi. Claude AI bilan MCP orqali ulanadi.</b></p>

ZIYNAT ochiq kodli [PhotoCraft](https://github.com/storytold/photocraft) (Rust, MIT/Apache-2.0)
asosida yaratilgan. Asl loyiha haqidagi to'liq ma'lumot: [README-PhotoCraft.md](README-PhotoCraft.md).

## Yuklab olish (Windows)

**Actions → Windows build** bo'limida oxirgi muvaffaqiyatli ishga tushirishni oching va
`ZIYNAT-…-windows-x64` artefaktini yuklab oling. Ichida:

- `ZIYNAT-<versiya>-windows-x64-setup.exe` — o'rnatuvchi;
- `ZIYNAT-<versiya>-windows-x64-portable.zip` — o'rnatmasdan ishlatish (fleshka uchun).

`v1.0.0` kabi teg qo'yilsa, ikkala fayl **Releases** bo'limida ham chiqadi.

## Claude AI bilan ishlash

ZIYNAT'dagi 500 dan ortiq buyruqning har birini Claude bajara oladi: qatlam yaratish,
filtrlar, rang tuzatish, matn, eksport va hokazo.

1. ZIYNAT'ni oching — pastdagi holat qatorida **Claude AI: ready**.
2. **Edit › Preferences › Integrations** → **Connect to Claude Desktop** (yoki **Connect to Claude Code**).
3. Claude Desktop'ni qayta oching. Ulanganda holat **Claude AI: connected** bo'ladi.

ZIYNAT yopiq bo'lsa, Claude ulanganda u o'zi ochiladi (`ziynat-cli mcp --launch-app`).

### Ruxsatlar

| Sozlama | Nima qiladi |
|---|---|
| Allow Agent Control | Claude ulanishini yoqadi/o'chiradi |
| Claude Auto Start | Har ishga tushganda `127.0.0.1:47820` portini ochadi |
| Agent Permission | `full` · `editOnly` (fayl ochish/saqlash/eksport/chop etish yo'q) · `readOnly` (faqat ko'rish) |
| Blocked Commands | Taqiqlangan buyruqlar: `file.print, filter.*, edit.preferences.` |
| Read / Write Folder | Claude fayl o'qishi/yozishi mumkin bo'lgan papkalar |

Ulanish faqat shu kompyuter ichida ishlaydi va 256-bitli token bilan himoyalangan.
Claude o'zining ruxsat sozlamalarini o'zgartira olmaydi.

Qo'lda sozlash (istalgan MCP mijoz uchun):

```json
{"mcpServers": {"ziynat": {
  "command": "C:\\Program Files\\ZIYNAT\\ziynat-cli.exe",
  "args": ["mcp", "--bridge", "127.0.0.1:47820",
           "--control-token-file", "C:\\Users\\<siz>\\AppData\\Roaming\\ZIYNAT\\claude-control.token",
           "--launch-app", "C:\\Program Files\\ZIYNAT\\ZIYNAT.exe"]}}}
```

## Manba koddan yig'ish

```sh
cargo build --release -p photocraft -p photocraft-cli --features photocraft/heif,photocraft-cli/heif
# target/release/photocraft(.exe) — ZIYNAT, photocraft-cli(.exe) — ziynat-cli
```

Windows'da Rust MSVC va Visual Studio Build Tools ("Desktop development with C++") kerak.

## Litsenziya

MIT yoki Apache-2.0 (tanlovingizga ko'ra). [NOTICE](NOTICE) va [ATTRIBUTION.md](ATTRIBUTION.md)
fayllaridagi asl mualliflik ma'lumotlari saqlanishi shart.
