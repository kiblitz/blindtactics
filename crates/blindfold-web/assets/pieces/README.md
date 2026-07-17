# Piece artwork

Cburnett's chess pieces, by Colin M.L. Burnett, as shipped by Lichess
(`lichess-org/lila`, `public/piece/cburnett`).

Licensed **GPLv2-or-later**, per lila's own `COPYING.md`
(`public/piece/cburnett | Colin M.L. Burnett | GPLv2+`). "Or later" is what makes
this [compatible][gpl] with the project, which is GPL-3.0-or-later already
because shakmaty forces it: we take the artwork under GPL-3.0.

This file said "GPL-3.0" until a review checked it against the source. The
conclusion was right and the cited fact was wrong — in the file whose whole job
is to state that fact correctly.

Attribution is required and is given in the app's footer, not only here: the
footer is what a user of the deployed site actually sees.

Fetched verbatim and unmodified. Their colour comes from the artwork itself —
every white piece is `fill="#fff"` over a black stroke, and the black pieces are
black, some by an explicit `fill="#000"` (bK, bB, bN) and some by letting the fill
default (bQ, bR, bP). Most of them also carry `#ececec` inner highlights (all but
bP). Either way they are black-on-black against the void, which is why the board
draws real light and dark squares on the reveal rather than staying flat: the
reveal is what makes them legible.

[gpl]: https://www.gnu.org/licenses/gpl-3.0.html
