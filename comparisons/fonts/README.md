# The comparisons' font

Every comparison adapter, in every scene, draws its text in this one file:
`DejaVuSans.ttf` from DejaVu Fonts 2.37
(https://github.com/dejavu-fonts/dejavu-fonts/releases/tag/version_2_37,
`dejavu-fonts-ttf-2.37.tar.bz2`), under its license (`LICENSE`: Bitstream Vera
and Arev, DejaVu's changes public domain).

- sha256 `7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954`
- FNV-1a 64 `ce252980ec1255ff` (what the adapters report: `font_file_fnv64=…`)

The runner copies it beside the staged apps and passes its path as
`ZGX_FONT_FILE`; each adapter loads exactly that file into its text system,
names the family `DejaVu Sans`, and prints `font_family=DejaVu Sans
font_file_fnv64=<the FNV-1a 64 of the bytes it loaded>` once at start. A trial
whose report is missing or names another file is rejected.
