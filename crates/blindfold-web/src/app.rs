//! The root component: holds the state, wires the panels to the board.

use crate::board;
use crate::chime;
use crate::database;
use crate::line;
use crate::panel;
use crate::rating;
use crate::recognition;
use crate::session;
use crate::settings;
use crate::speech;
use crate::square;
use blindfold_core::arrow;
use blindfold_core::diction;
use blindfold_core::roster;
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    let session = RwSignal::new(session::Session::new(database::load()));
    // The user's puzzle Elo, loaded from localStorage (or the starting value), and
    // the change from the last scored puzzle so it can be shown like chess.com's
    // "+8". The rating steers which puzzle comes next, so seat the opening one near
    // it rather than always landing on index 0.
    let elo = RwSignal::new(rating::load());
    let elo_delta = RwSignal::new(None::<i32>);
    session.update(|s| s.reseat(elo.get_untracked(), js_sys::Math::random()));

    // Which side faces the user, persisted across reloads. The per-puzzle flip that
    // layers on top of it lives in the `attempt` below, transient by design.
    let pov = RwSignal::new(settings::load_pov());

    // Voice mode's two halves, each persisted. `input` decides whether the mic arms
    // (and how it carries across puzzles); `output` decides whether the roster and
    // verdict are read aloud *automatically*. Both default off/visual: audio needs a
    // user gesture to start, and a page that talks or listens on load is a surprise.
    let input_mode = RwSignal::new(settings::load_input());
    let output = RwSignal::new(settings::load_output());
    // Start the browser loading its voice list now, so a good voice is chosen from the
    // very first announcement rather than after it (some browsers load voices async).
    speech::warm();
    // If the user already prefers Speak (persisted from a prior visit), start pulling the
    // recogniser's ~41 MB model in the background now, so their first record tap activates
    // the mic immediately instead of waiting on the download. Only on that intent — a
    // Draw-mode or first-time visitor never pays for a model they may not use.
    if matches!(input_mode.get_untracked(), settings::Input::Audio) {
        recognition::warm();
    }

    // Whether the mic is actually running, and the user's *last-set* intent for it.
    // The two differ because the app pauses the running recogniser for its own speech
    // (see `speech::say`) without the user turning anything off, and because audio
    // mode re-arms the mic on a new puzzle from the last intent (see `Input::arms_next`).
    let listening = RwSignal::new(false);
    let mic_desired = RwSignal::new(false);
    // The provisional arrow streamed onto the board while the user is still speaking a
    // move — shown ghosted, replaced by a committed arrow once the move is settled.
    let preview = RwSignal::new(None::<arrow::Arrow>);
    // Streaming multi-move state. A `continuous` recogniser hands back a growing
    // transcript that may hold several moves ("queen f5 queen g6"); `committed` counts how
    // many of the current utterance's parsed moves we have already drawn, so a growing
    // interim only draws the *new* ones. `heard_so_far` is the previous transcript: while
    // each event *extends* it the count carries forward, but when a transcript no longer
    // starts with it the recogniser has restarted fresh (Chrome ends a session on its own
    // silence and reopens) and the count resets to zero. Resetting on *every* final instead
    // would double-draw the earlier moves whenever the user speaks with a gap long enough
    // to finalise a segment mid-line. See "Voice input" in CLAUDE.md.
    let committed = RwSignal::new(0usize);
    let heard_so_far = RwSignal::new(String::new());

    // The whole attempt in one signal, so its reset invariant lives in one place a
    // native test can reach — see `session::Attempt`.
    let attempt = RwSignal::new(session::Attempt::new());

    // `Memo`, not `Signal::derive`: each caches its value and clones only the
    // current puzzle (via `session.with`), not the whole `Session`, and only
    // recomputes when the puzzle actually changes.
    //
    // The `position` memo dedups on `shakmaty::Chess`, whose `PartialEq` ignores the
    // clocks and a non-capturable ep square (see CLAUDE.md). Safe here: those are
    // exactly the fields the roster omits and the answer does not turn on, so a
    // dedup that treats two clock-only-different positions as equal shows an
    // identical board — and judging reads the `puzzle` memo, not this one.
    let puzzle = Memo::new(move |_| session.with(|s| s.current().clone()));
    let position = Memo::new(move |_| {
        puzzle.with(|p| p.position().expect("the embedded database is verified"))
    });
    let solver =
        Memo::new(move |_| puzzle.with(|p| p.solver().expect("the embedded database is verified")));

    // The attempt projected for the view. `Memo`, not a plain derive, so stepping
    // the reveal — which changes `attempt` but not the drawn line or the verdict —
    // does not re-render the line panel or, worse, re-announce the verdict under
    // its `aria-live`. The reveal signals below are memos for the same reason: the
    // pieces layer redraws only when the position it shows actually changes.
    let drawn = Memo::new(move |_| attempt.with(|a| a.arrows().to_vec()));
    let solve = Memo::new(move |_| attempt.with(|a| a.solve().cloned()));
    // Revealed (solve or give-up) locks the board and swaps the panel to the
    // solution. `ply` is the reveal cursor the move list highlights.
    let locked = Memo::new(move |_| attempt.with(session::Attempt::is_revealed));
    let ply = Memo::new(move |_| attempt.with(session::Attempt::ply));
    // The reveal as a move list. Depends on `solve` and the puzzle, not on `ply`, so
    // it is built once per reveal and not rebuilt on every step. `None` until the
    // board is revealed.
    let movelist = Memo::new(move |_| {
        solve.with(|s| {
            s.as_ref()
                .and_then(session::Solve::steps)
                .map(|steps| position.with(|p| session::movelist(p, steps)))
        })
    });
    let can_back = Memo::new(move |_| attempt.with(session::Attempt::can_step_back));
    let can_forward = Memo::new(move |_| attempt.with(session::Attempt::can_step_forward));
    // A `Memo` so the board rebuilds when the flip toggles but *not* on every arrow
    // draw — reading `attempt` directly in the board's render would resubscribe it to
    // the whole attempt and rebuild the board (losing an in-progress drag) each edit.
    let flipped = Memo::new(move |_| attempt.with(session::Attempt::flipped));

    let next = move |_| {
        let r = js_sys::Math::random();
        let rating_now = elo.get_untracked();
        session.update(|s| s.advance(rating_now, r));
        attempt.update(session::Attempt::reset);
        elo_delta.set(None);
    };

    // Apply a scoring outcome to the rating — shared by submit and give-up, since
    // both move the rating exactly the same way and `Attempt` already decides *whether*
    // a given event scores (returning `Some` only for the first one on the puzzle). A
    // `Callback` so both closures can hold it. `None` is the common no-score case.
    let score = Callback::new(move |outcome: Option<rating::Outcome>| {
        let Some(outcome) = outcome else {
            return;
        };
        let before = elo.get_untracked();
        let puzzle_rating = puzzle.with_untracked(|p| p.rating);
        let after = rating::update(before, puzzle_rating, outcome);
        elo.set(after);
        // No badge when the rating did not move — reachable only at the floor or
        // ceiling, where a win/loss clamps back to where it was, and a "+0" up-badge
        // there would read as a gain that did not happen. Both are <= ELO_CEILING
        // (3000), so the i32 cast is exact.
        elo_delta.set((after != before).then(|| after as i32 - before as i32));
        rating::save(after);
    });

    let submit = move |_| {
        // Prime the chime's audio context while we (may) still be inside the click
        // gesture, so the verdict's tone is not swallowed by a suspended context. A
        // no-op when driven from a spoken "submit", where the mic tap already warmed it.
        chime::warm();
        let line = attempt.with_untracked(|a| a.arrows().to_vec());
        let verdict = puzzle.with_untracked(|p| session::solve(p, &line));
        // `submit` returns the outcome only for the first scoring submission on the
        // puzzle, so a miss-then-solve or a re-solve does not move the rating twice.
        let outcome = attempt.try_update(|a| a.submit(verdict)).flatten();
        score.run(outcome);
    };

    // Give up: reveal the puzzle's *own* stored solution (there is no winning line of
    // the user's to play out) and score it as a loss, once. `give_up` returns the
    // outcome under the same first-event-only rule `submit` uses.
    let give_up = move |()| {
        chime::warm();
        let steps = puzzle.with_untracked(session::reveal);
        let outcome = attempt.try_update(|a| a.give_up(steps)).flatten();
        score.run(outcome);
    };

    let draw = move |arrow: arrow::Arrow| attempt.update(|a| a.draw(arrow));
    let flip = move |()| attempt.update(session::Attempt::flip);

    // The settings write-and-persist closures, lifted here rather than in the menu so
    // the settings component stays markup-and-plumbing — the same lifted-callback shape
    // as `submit`. Each is guarded against a no-op re-click so it does not needlessly
    // churn state (and, for the POV, rebuild the board mid-drag). **Selecting a mode
    // deliberately actuates nothing** — no speech starts, no mic arms — it only changes
    // what happens on the *next* puzzle; that is why none of these touch `listening`.
    let choose_pov = move |chosen: settings::Pov| {
        if pov.get_untracked() != chosen {
            pov.set(chosen);
            settings::save_pov(chosen);
        }
    };
    let choose_input = move |chosen: settings::Input| {
        if input_mode.get_untracked() != chosen {
            input_mode.set(chosen);
            settings::save_input(chosen);
            // Switching to Speak is a strong signal the mic is about to be used, so start
            // pulling the ~41 MB model now (no mic, no gesture) — by the first record tap it
            // is in memory and the mic activates at once rather than stalling on the download.
            if matches!(chosen, settings::Input::Audio) {
                recognition::warm();
            }
        }
    };
    let choose_output = move |chosen: settings::Output| {
        if output.get_untracked() != chosen {
            output.set(chosen);
            settings::save_output(chosen);
        }
    };
    let undo = move |()| attempt.update(session::Attempt::undo);
    let clear = move |()| attempt.update(session::Attempt::clear);
    let promote = move |(index, role): (usize, Option<shakmaty::Role>)| {
        attempt.update(|a| a.set_promotion(index, role))
    };
    let step_back = move |()| attempt.update(session::Attempt::step_back);
    let step_forward = move |()| attempt.update(session::Attempt::step_forward);
    let step_to = move |ply: usize| attempt.update(|a| a.step_to(ply));

    // Read the current puzzle's roster aloud. `speech::say` deafens the recogniser for
    // the read (the echo guard), so this doubles as the mechanism that holds the mic off
    // while the roster plays — the shared core of three call sites: the roster panel's
    // speak button, the spoken "repeat" command, and the new-puzzle auto-read.
    let say_roster = move || {
        speech::say(&position.with_untracked(|p| roster::of(p).speech()));
    };
    // The roster panel's speak button — a toggle, not a restart. If the roster (or any
    // read-aloud) is already playing, a press stops it; otherwise it reads the roster.
    // Always available regardless of the output mode: the setting only governs
    // *automatic* reading, this is the user asking for it (and the click is the gesture
    // browsers require before speech). The spoken "skip" command is the voice sibling.
    let speak_roster = move |()| {
        if speech::is_speaking() {
            speech::silence();
        } else {
            say_roster();
        }
    };

    // --- voice input ---------------------------------------------------------
    //
    // A heard transcript becomes a drawn arrow, an app command, or a spoken reply —
    // all decided by `session::interpret` (native-tested), against the line played
    // forward through the stored defenses. Here we only carry the verdict out, reusing
    // the very same action closures the buttons use so the two paths cannot diverge.
    let mic_supported = recognition::is_supported();

    // Turn the mic off — the mechanical stop. Does *not* touch `mic_desired`: the record
    // toggle and only the record toggle records the user's intent, so a new-puzzle reset
    // does not look like the user disarming (which in audio mode would stop it re-arming
    // next puzzle).
    let deafen = move || {
        recognition::stop();
        listening.set(false);
        preview.set(None);
    };

    let handle_voice = move |transcript: String, is_final: bool| {
        // A transcript that no longer extends the last one is a fresh recogniser session
        // (Chrome closed the previous on its own silence and reopened), so its moves count
        // from zero. A transcript that *does* extend the last one is the same line still
        // growing — keep the count, so already-drawn moves are not drawn again.
        if !transcript.starts_with(&heard_so_far.get_untracked()) {
            committed.set(0);
        }
        heard_so_far.set(transcript.clone());

        let parsed = diction::parse_line(&transcript);
        // Voice input is the most bug-prone part of the app and cannot be e2e-tested (no
        // recognition in headless chromium), so a console trail of exactly what the
        // browser heard, and how it segmented, is the only way to diagnose a mishear.
        leptos::logging::log!(
            "voice: heard {transcript:?} final={is_final} -> {:?} trailing={}",
            parsed.intents,
            parsed.trailing,
        );

        // Confirm-on-next: a complete move is drawn once another segment follows it, or
        // on a final. During an interim the *last* complete move is held back — it may
        // still be revised as the user keeps speaking — and only shown as a preview.
        let confirmed = if parsed.trailing || is_final {
            parsed.intents.len()
        } else {
            parsed.intents.len().saturating_sub(1)
        };

        // Draw the newly-confirmed moves, each against the position it is made from (the
        // line grown by the moves before it). Stop at anything that needs the user — an
        // ambiguity, a promotion, an illegal move — or, on an interim, a command.
        let mut idx = committed.get_untracked().min(parsed.intents.len());
        while idx < confirmed {
            match &parsed.intents[idx] {
                diction::Intent::Command(command) => {
                    // Commands act only on a final — a mid-utterance "submit" must not fire.
                    if !is_final {
                        break;
                    }
                    match command {
                        diction::Command::Submit => submit(()),
                        diction::Command::Undo => undo(()),
                        diction::Command::Clear => clear(()),
                        diction::Command::Next => next(()),
                        diction::Command::GiveUp => give_up(()),
                        // Re-read the roster. `say_roster` deafens the mic for the read
                        // (the echo guard), so "repeat" turns the mic off, plays the
                        // roster, and resumes.
                        diction::Command::Repeat => say_roster(),
                        // Cut a roster read short. `speech::silence` cancels it, and the
                        // resume poll un-deafens the mic once the synth goes idle. (A read
                        // in progress has the mic deafened, so this fires from a live mic
                        // between reads — the roster speak button is the touch sibling.)
                        diction::Command::Skip => speech::silence(),
                    }
                    idx += 1;
                }
                intent => {
                    let heard = puzzle.with_untracked(|p| {
                        attempt.with_untracked(|a| session::resolve_spoken(intent, p, a.arrows()))
                    });
                    match heard {
                        session::Heard::Draw { arrow } => {
                            attempt.update(|a| a.draw(arrow));
                            // A short ding confirms each move the mic understood and drew
                            // — the audio counterpart of the arrow appearing, so a
                            // hands-free solver hears their line building without looking.
                            chime::correct();
                            idx += 1;
                        }
                        // A question or miss: speak it (on a final only, so an interim does
                        // not repeat it every word) and wait — do not race past it.
                        session::Heard::Say(text) => {
                            if is_final {
                                speech::say(&text);
                            }
                            break;
                        }
                        session::Heard::Command(_) => {
                            unreachable!("a move intent never resolves to a command")
                        }
                    }
                }
            }
        }
        committed.set(idx);

        // Preview the move still being spoken: the last unconfirmed complete move,
        // ghosted, when it resolves to an arrow. Suppressed once the board is revealed —
        // the mic stays on after a solve (to catch "next"), but a stray move word then
        // must not flash a ghost on the answer.
        let revealed = attempt.with_untracked(session::Attempt::is_revealed);
        let ghost = (!revealed && idx < parsed.intents.len())
            .then(|| {
                puzzle.with_untracked(|p| {
                    attempt.with_untracked(|a| {
                        session::resolve_spoken(&parsed.intents[idx], p, a.arrows())
                    })
                })
            })
            .and_then(|heard| match heard {
                session::Heard::Draw { arrow } => Some(arrow),
                _ => None,
            });
        preview.set(ghost);
    };

    // Start listening, returning whether it actually started. The browser may refuse (no
    // permission / no gesture), in which case the caller must not leave the control armed.
    let start_listening = move || {
        if recognition::start(handle_voice) {
            listening.set(true);
            // Fresh session: the streaming move counter and heard-transcript start over.
            committed.set(0);
            heard_so_far.set(String::new());
            true
        } else {
            false
        }
    };

    // The record control: arm or disarm the mic, recording the user's intent so audio
    // mode carries it across puzzles. Arming does *not* read the roster — the puzzle is
    // read only by the output setting's auto-read or the roster's speak button (the
    // user's call: turning on the mic should not, on its own, start talking). The tap is
    // still a user gesture, so it unlocks the browser's speech for a later verdict.
    let toggle_mic = move |()| {
        // The tap is a user gesture — prime the chime's audio context now so the
        // per-move dings and the verdict tone are not swallowed by a suspended context.
        chime::warm();
        if listening.get_untracked() {
            mic_desired.set(false);
            deafen();
        } else {
            mic_desired.set(true);
            // Refused (no permission / no gesture): do not leave the button armed over a
            // mic that never started.
            if !start_listening() {
                mic_desired.set(false);
            }
        }
        leptos::logging::log!(
            "voice: toggle_mic -> desired={}",
            mic_desired.get_untracked()
        );
    };

    // Arrow keys walk the reveal, like a Lichess analysis board — only while revealed,
    // so they never fight anything during solving. `prevent_default` stops the page
    // scrolling out from under the board on the same key. The listener is on the
    // window (there is nothing focusable to hang it on) and removed on cleanup.
    let keydown = window_event_listener(leptos::ev::keydown, move |ev| {
        if !attempt.with_untracked(session::Attempt::is_revealed) {
            return;
        }
        match ev.key().as_str() {
            "ArrowLeft" => {
                ev.prevent_default();
                attempt.update(session::Attempt::step_back);
            }
            "ArrowRight" => {
                ev.prevent_default();
                attempt.update(session::Attempt::step_forward);
            }
            _ => {}
        }
    });
    on_cleanup(move || keydown.remove());

    // Set a *new* puzzle up for voice: reset the streaming state, arm or disarm the mic
    // per the input mode, then (if the output speaks) read the roster aloud. Subscribes
    // to `puzzle` only (fires on load and every `next`) and reads the modes and the last
    // mic intent untracked, so toggling a mode mid-puzzle actuates nothing — this fires
    // when the puzzle changes, which is when re-arming is wanted.
    //
    // The order is load-bearing: arm the mic *before* reading the roster. `say` deafens
    // the recogniser for the read (the echo guard), so a just-armed mic is held off for
    // the whole roster and only starts hearing on the utterance's `end`. Reading first
    // would leave a window where the mic could come up mid-roster and hear it back.
    // (In audio mode a mic already running keeps running — no restart, no gap; in draw
    // mode a mic left on from the previous puzzle is turned off.)
    Effect::new(move |_| {
        puzzle.track();
        // A fresh line: reset the streaming counters — the new empty line has no move in
        // yet, and a mic carried across puzzles (audio mode) starts the line over.
        committed.set(0);
        heard_so_far.set(String::new());
        preview.set(None);

        let want = input_mode
            .get_untracked()
            .arms_next(mic_desired.get_untracked());
        let now = listening.get_untracked();
        let speaks = output.get_untracked().speaks();
        leptos::logging::log!("voice: new puzzle -> want_mic={want} now={now} speaks={speaks}");
        if want && !now {
            start_listening();
        } else if !want && now {
            deafen();
        }

        // Auto-read the roster if the output mode speaks. On the very first run the browser
        // may refuse for lack of a gesture; that degrades to silence until the first
        // interaction, which is acceptable. `speaks` was read untracked above — switching
        // the mode must not re-read the roster (selecting a setting actuates nothing).
        if speaks {
            say_roster();
        }
    });

    // Sound the moment a submission or give-up produces a verdict, and speak it. Subscribes
    // to `solve` only: it fires when the verdict appears, not when the reveal is stepped
    // (that changes `ply`, not `solve`) and not when a mode is toggled (read untracked).
    //
    // The chime plays in *any* mode — it is instant feedback, not reading aloud, so the
    // output setting does not govern it. A solved mate dings; every scored loss (a miss, an
    // overshoot, or a give-up) buzzes; the unscored results (an incomplete promotion, an
    // unjudged line) make no sound. The spoken sentence is the detailed sibling, and it
    // *is* gated on the read-aloud mode.
    Effect::new(move |_| {
        if let Some(verdict) = solve.get() {
            match &verdict {
                session::Solve::Solved(_) => chime::correct(),
                session::Solve::GaveUp(_)
                | session::Solve::Overshot { .. }
                | session::Solve::Refuted { .. } => chime::wrong(),
                session::Solve::Incomplete(_) | session::Solve::Unjudged(_) => {}
            }
            if output.get_untracked().speaks() {
                speech::say(&session::spoken(&verdict, solver.get_untracked()));
            }
        }
    });

    // The position the board draws: `None` while the user is still blind, then the
    // start position, then each ply the reveal has been stepped to. Stepped by hand
    // through the line panel's controls — there is no timer.
    let revealed = Memo::new(move |_| {
        attempt.with(|a| {
            a.steps().map(|steps| {
                session::step_at(steps, a.ply())
                    .map_or_else(|| position.get(), |step| step.after.clone())
            })
        })
    });

    // The square the move just landed on.
    //
    // Via `Arrow::of_move` rather than `Move::to()`, which for a castle returns
    // the **rook's** square — so the reveal would light h1 while the king walked
    // to g1. CLAUDE.md lists this among the shakmaty gotchas that cost real time,
    // and `of_move` exists precisely to spell a move the way a drag would.
    let highlight = Memo::new(move |_| {
        attempt.with(|a| {
            a.steps()
                .and_then(|steps| session::step_at(steps, a.ply()))
                .and_then(|step| arrow::Arrow::of_move(&step.played))
                .map(|drag| drag.to)
        })
    });

    view! {
        <main class="app">
            <header class="masthead">
                <p class="masthead__eyebrow">"Blindfold chess trainer"</p>
                <h1 class="masthead__title">"The board stays empty."</h1>
                <p class="masthead__lede">
                    "You get a roster of squares and nothing else. Draw your line — one arrow per
                     move of your own side, in order — then submit. Solve it and the pieces appear."
                </p>
            </header>

            // One header row: the rating on the left, the board's view controls (flip,
            // settings) on the right — the toolbar for everything that is not the board
            // itself. Putting the view controls here rather than in a bar above the
            // board frees that whole row for the board on a phone.
            <div class="topbar">
                <RatingBar rating=elo delta=elo_delta />
                <BoardBar
                    pov=pov
                    flipped=flipped
                    input=input_mode
                    output=output
                    on_flip=Callback::new(flip)
                    on_choose_pov=Callback::new(choose_pov)
                    on_choose_input=Callback::new(choose_input)
                    on_choose_output=Callback::new(choose_output)
                />
            </div>

            <div class="layout">
                <div class="layout__board">
                    // A frame around the board so it can be a size container on a
                    // phone: the board then fills the smaller of the frame's width and
                    // height, shrinking to fit the space the fixed-height mobile shell
                    // leaves it rather than forcing a scroll. On desktop the frame is
                    // `display: contents` — a passthrough that changes nothing.
                    <div class="board-frame">
                        {move || {
                            // Rebuilt when the puzzle, the POV, or the flip changes — a
                            // fresh board without the last render's in-progress drag. The
                            // `puzzle.track()` is load-bearing beyond the orientation read:
                            // two puzzles can resolve to the *same* orientation, and
                            // without it the board would carry the previous drag into the
                            // next puzzle. `track`, not `get`: subscribe without cloning.
                            puzzle.track();
                            let side = settings::facing(pov.get(), solver.get(), flipped.get());
                            view! {
                                <board::Board
                                    orientation=square::Orientation(side)
                                    drawn=drawn
                                    preview=preview
                                    on_draw=Callback::new(draw)
                                    revealed=revealed
                                    highlight=highlight
                                    locked=locked
                                />
                            }
                        }}
                    </div>
                    <Facts session=session />
                </div>

                <aside class="layout__panels">
                    {move || {
                        // Keyed on the puzzle, like the board above, so the roster
                        // and line rebuild fresh when a new one is served.
                        puzzle.track();
                        let r = Signal::derive(move || roster::of(&position.get()));
                        // Wrapped so each is an independent layout region: on a phone
                        // the roster is hoisted above the board and pinned there (see
                        // `.layout__roster` in styles.css), the line stays below.
                        view! {
                            <div class="layout__roster">
                                <panel::Roster roster=r on_speak=Callback::new(speak_roster) />
                            </div>
                            <div class="layout__line">
                            <line::Line
                                drawn=drawn
                                solver=solver.get()
                                solve=solve
                                movelist=movelist
                                ply=ply
                                can_back=can_back
                                can_forward=can_forward
                                mic_supported=mic_supported
                                listening=listening
                                on_toggle_mic=Callback::new(toggle_mic)
                                on_undo=Callback::new(undo)
                                on_clear=Callback::new(clear)
                                on_submit=Callback::new(submit)
                                on_give_up=Callback::new(give_up)
                                on_promote=Callback::new(promote)
                                on_next=Callback::new(next)
                                on_step_back=Callback::new(step_back)
                                on_step_forward=Callback::new(step_forward)
                                on_step_to=Callback::new(step_to)
                            />
                            </div>
                        }
                    }}
                </aside>
            </div>

            <footer class="colophon">
                <p>
                    "Every puzzle here is re-proved by the same solver the browser runs: a forced
                     mate against "<em>"every"</em>" defense, with a roster small enough to hold —
                     four to ten squares. Your line is played out, not compared to a stored answer,
                     so any mate you find counts."
                </p>
                <p>
                    "Puzzles from the "
                    <a href="https://database.lichess.org/#puzzles">"Lichess puzzle database"</a>
                    " (CC0). Pieces by Colin M.L. Burnett, via Lichess (GPLv2-or-later). This app is "
                    <a href="https://www.gnu.org/licenses/gpl-3.0.html">"GPL-3.0-or-later"</a> "."
                </p>
            </footer>
        </main>
    }
}

/// The user's rating and the change from the last scored puzzle.
///
/// Chess.com-style: the rating with a "+8" / "-6" badge beside it that appears only
/// after a scored submission and clears on the next puzzle. The pool size used to sit
/// here too; it was noise — a user does not need a running count of the database.
#[component]
fn RatingBar(
    #[prop(into)] rating: Signal<u32>,
    /// The signed change from the last scored puzzle, or `None` when nothing has
    /// been scored since the current puzzle was served.
    #[prop(into)]
    delta: Signal<Option<i32>>,
) -> impl IntoView {
    view! {
        <div class="statusbar">
            <span class="elo">
                "Rating " <strong>{move || rating.get().to_string()}</strong>
                {move || {
                    delta
                        .get()
                        .map(|d| {
                            let up = d >= 0;
                            let sign = if up { "+" } else { "" };
                            view! {
                                <span
                                    class="elo__delta"
                                    class:elo__delta--up=up
                                    class:elo__delta--down=!up
                                >
                                    {format!("{sign}{d}")}
                                </span>
                            }
                        })
                }}
            </span>
        </div>
    }
}

/// The puzzle's id, small and muted under the board.
///
/// Kept only so a user who hits something odd can report *which* puzzle, and look
/// it up in the committed database. Everything else that used to sit here is gone
/// on purpose: the rating is a difficulty hint, the square count read as clutter,
/// and the mate depth is the very thing the trainer withholds.
#[component]
fn Facts(session: RwSignal<session::Session>) -> impl IntoView {
    view! {
        <p class="facts mono">{move || format!("#{}", session.with(|s| s.current().id.clone()))}</p>
    }
}

/// The board's view controls: flip, and the settings menu.
///
/// Both change *how* the board is drawn or delivered, not what the puzzle is, so they
/// sit in the header row (`.topbar`) with the rating — the toolbar for everything that
/// is not the board — rather than in a bar above the board, which on a phone is a row
/// of height the board would rather have. The flip is a transient per-puzzle toggle
/// (its `pressed` state tracks the attempt); the settings menu holds the persisted
/// preferences (point of view, and voice mode's input/output modes).
#[component]
fn BoardBar(
    #[prop(into)] pov: Signal<settings::Pov>,
    #[prop(into)] flipped: Signal<bool>,
    #[prop(into)] input: Signal<settings::Input>,
    #[prop(into)] output: Signal<settings::Output>,
    #[prop(into)] on_flip: Callback<()>,
    #[prop(into)] on_choose_pov: Callback<settings::Pov>,
    #[prop(into)] on_choose_input: Callback<settings::Input>,
    #[prop(into)] on_choose_output: Callback<settings::Output>,
) -> impl IntoView {
    view! {
        <div class="boardbar">
            <button
                class="button button--icon"
                class:button--on=move || flipped.get()
                aria-pressed=move || flipped.get().to_string()
                aria-label="Flip board"
                title="Flip board"
                on:click=move |_| on_flip.run(())
            >
                "⇅"
            </button>
            <Settings
                pov=pov
                input=input
                output=output
                on_choose_pov=on_choose_pov
                on_choose_input=on_choose_input
                on_choose_output=on_choose_output
            />
        </div>
    }
}

/// A labelled radio group inside the settings menu: a heading and one button per
/// option, the current one pressed. Factored out because the three mode groups (point
/// of view, input, output) render identically — a heading, options off `ALL`, one
/// checked — differing only in their labels and which index a click picks. `on_pick`
/// takes the option's index into its `ALL`, which the caller maps back to the enum.
fn radio_group(
    heading: &'static str,
    options: Vec<(&'static str, Signal<bool>)>,
    on_pick: Callback<usize>,
) -> impl IntoView {
    view! {
        <p class="settings__heading">{heading}</p>
        {options
            .into_iter()
            .enumerate()
            .map(|(i, (label, on))| {
                view! {
                    <button
                        class="settings__option"
                        class:settings__option--on=move || on.get()
                        role="menuitemradio"
                        aria-checked=move || on.get().to_string()
                        on:click=move |_| on_pick.run(i)
                    >
                        {label}
                    </button>
                }
            })
            .collect_view()}
    }
}

/// The settings menu: a gear that opens a small panel of preferences.
///
/// Point of view, and voice mode's two modes. Toggled open by the gear; the board and
/// modes update live behind it as choices change, so there is nothing to confirm and no
/// need to close on select.
#[component]
fn Settings(
    #[prop(into)] pov: Signal<settings::Pov>,
    #[prop(into)] input: Signal<settings::Input>,
    #[prop(into)] output: Signal<settings::Output>,
    #[prop(into)] on_choose_pov: Callback<settings::Pov>,
    #[prop(into)] on_choose_input: Callback<settings::Input>,
    #[prop(into)] on_choose_output: Callback<settings::Output>,
) -> impl IntoView {
    let open = RwSignal::new(false);

    // Each mode group's options as (label, is-current), read off its `ALL` so the menu
    // cannot drift from the enum. The `on_pick` callbacks map an index back to `ALL`.
    let pov_options = move || {
        settings::Pov::ALL
            .into_iter()
            .map(|option| (option.label(), Signal::derive(move || pov.get() == option)))
            .collect::<Vec<_>>()
    };
    let input_options = move || {
        settings::Input::ALL
            .into_iter()
            .map(|option| {
                (
                    option.label(),
                    Signal::derive(move || input.get() == option),
                )
            })
            .collect::<Vec<_>>()
    };
    let output_options = move || {
        settings::Output::ALL
            .into_iter()
            .map(|option| {
                (
                    option.label(),
                    Signal::derive(move || output.get() == option),
                )
            })
            .collect::<Vec<_>>()
    };
    let pick_pov = Callback::new(move |i: usize| on_choose_pov.run(settings::Pov::ALL[i]));
    let pick_input = Callback::new(move |i: usize| on_choose_input.run(settings::Input::ALL[i]));
    let pick_output = Callback::new(move |i: usize| on_choose_output.run(settings::Output::ALL[i]));

    view! {
        <div class="settings">
            <button
                class="button button--icon"
                aria-haspopup="menu"
                aria-expanded=move || open.get().to_string()
                aria-label="Settings"
                title="Settings"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                "⚙"
            </button>
            {move || {
                open.get()
                    .then(|| {
                        view! {
                            <div class="settings__menu" role="menu" aria-label="Settings">
                                {radio_group("Point of view", pov_options(), pick_pov)}
                                {radio_group("Input", input_options(), pick_input)}
                                {radio_group("Output", output_options(), pick_output)}
                            </div>
                        }
                    })
            }}
        </div>
    }
}
