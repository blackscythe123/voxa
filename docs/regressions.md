# Regressions: every bug and every decision, and what guards it

The owner's rule (2026-11-28): every bug found and every decision made is locked in by an
automated check, so it can never come back silently. This file is the index. A row without a guard
is a bug in this file.

How the guards run:

- **Rust tests**: `cargo test -q --release --workspace --no-fail-fast` (CI: `desktop.yml` on
  Windows and macOS; `guardrails.yml` runs the core crates on Linux).
- **Lints**: `python tools/lint/run.py` and `python tools/lint/run.py --self-test` (CI on every
  push; also the pre-commit hook). Rules are in `docs/guardrails.md`.
- **Emulator**: Android instrumented tests in `android/app/src/androidTest`, run on the emulator
  (`adb -s emulator-5554 shell am instrument -w com.voxa.android.test/androidx.test.runner.AndroidJUnitRunner`).
  They are not in CI yet (CI has no emulator), so they are run before every Android release.
- **Partly manual**: the decision is tested as a pure function or by reading the source, but the
  OS, GPU or timing leg it protects can only be seen on a real machine. What stays manual is said.

Path abbreviations: `cF` core/voxa-core/tests/flow.rs, `cR` core/voxa-core/tests/regressions.rs,
`cHD` core/voxa-core/tests/hunt_desktop.rs, `cDR` core/voxa-core/tests/desktop_regressions.rs,
`cTT` core/voxa-core/tests/trim_text.rs, `cG` core/voxa-core/src/greeting.rs, `cD`
core/voxa-core/src/doodle.rs, `cS` core/voxa-core/src/splash.rs, `cN` core/voxa-core/src/notices.rs,
`cE` core/voxa-core/src/error.rs, `gL` core/voxa-chatgpt/src/lib.rs, `gR`
core/voxa-chatgpt/tests/regressions.rs, `sR` core/voxa-store/tests/regressions.rs, `sRW`
core/voxa-store/tests/realworld.rs, `sL` core/voxa-store/src/lib.rs, `sA` core/voxa-store/src/audio.rs,
`fT` core/voxa-ffi/tests/ffi.rs, `fHR` core/voxa-ffi/tests/history_regressions.rs, `fKR`
core/voxa-ffi/tests/keyboard_regressions.rs, `fRW` core/voxa-ffi/tests/realworld.rs, `fAS`
core/voxa-ffi/tests/android_source.rs, `fG` core/voxa-ffi/tests/greeting.rs, `fL`
core/voxa-ffi/src/lib.rs, `fN` core/voxa-ffi/src/notices.rs, `fNu` core/voxa-ffi/src/nudge.rs,
`nR` core/voxa-nudge/tests/regressions.rs, `nRu` core/voxa-nudge/src/rules.rs, `nE`
core/voxa-nudge/src/engine.rs, `nS` core/voxa-nudge/src/state.rs, `nM` core/voxa-nudge/src/messages.rs,
`dM` desktop/app/src/main.rs, `dSR` desktop/app/src/strip_rules.rs, `dRu` desktop/app/src/rules.rs,
`dSt` desktop/app/src/store.rs, `dU` desktop/app/src/uploads.rs, `dRec` desktop/app/src/recover.rs,
`dW` desktop/app/src/win.rs, `dN` desktop/app/src/notify.rs, `dA` desktop/app/src/audio.rs, `dH`
desktop/app/src/home.rs, `dT` desktop/app/src/tab.rs, `dTk` desktop/app/src/talk.rs, `dTh`
desktop/app/src/theme.rs, `dRd` desktop/app/src/redact.rs, `dL` desktop/app/src/link.rs, `dSi`
desktop/signin/src/main.rs, `aR` androidTest RegressionsTest.kt, `aC` androidTest ContrastTest.kt,
`aK` androidTest KeyboardLookTest.kt, `aF` androidTest CoreFfiTest.kt, `lint:x` rule x of
tools/lint.

## Bugs

### Desktop strip, focus and windows

| Bug in plain words | Found | Fix | Guarding test or lint | Kind |
|---|---|---|---|---|
| Strip invisible after sleep or Modern Standby | 11-24 | cdb5e88, 6b15935 | dSR::sleep_charger_and_display_changes_mark_the_strip_for_a_rebuild, dSR::display_changes_only_mark_the_strip_and_the_shortcut_rebuilds_once | partly manual (a real sleep) |
| Strip invisible after a charger change or graphics chip switch (two-GPU laptop) | 11-28 | 6b15935 | dSR::sleep_charger_and_display_changes_mark_the_strip_for_a_rebuild | partly manual (a real GPU switch) |
| Rebuilding the strip on WM_DISPLAYCHANGE stole focus from full-screen games, in a loop | 11-24 | a5c11ad, 6b15935 | dSR::display_changes_only_mark_the_strip_and_the_shortcut_rebuilds_once, dSR::the_strip_is_rebuilt_only_from_the_shortcut_or_a_resume, lint:focus-steal | partly manual (a real game) |
| The wake rebuild still grabbed focus: GPUI showed a hidden window for 0.5 s | 11-24 | 8adeb5b | dM::the_strip_window_opens_without_focus, dSR::the_strip_is_never_shown_with_an_activating_flag, lint:focus-steal | partly manual |
| GPUI's display-change handler showed the hidden strip by itself (monitor unplug, RDP, KVM) | 11-28 | 08597bf | lint:focus-steal (only `no_activate_proc` may hand focus back), dSR::the_strip_is_rebuilt_only_from_the_shortcut_or_a_resume | partly manual (hide-again leg) |
| Every battery percentage change counted as a power change, rebuilding the strip at each press | 11-28 | 6b15935 | dSR::sleep_charger_and_display_changes_mark_the_strip_for_a_rebuild (only resume, power-source and display messages) | automated |
| A rebuilt strip kept off-screen bounds after a monitor or resolution change | 11-28 | 08597bf | dSR::a_rebuilt_strip_with_off_screen_bounds_goes_back_to_the_bottom_centre | automated |
| `strip_blank` stuck if `open_window` failed, leaving the strip invisible until restart | 11-24 | d1c85d2 | dSR::a_failed_rebuild_keeps_the_old_strip_and_never_leaves_it_blank | automated |
| Two strip rebuilds at once leaked a window | 11-28 | 86bc6d8 | dRu::only_one_strip_rebuild_runs_at_a_time | automated |
| "RefCell already borrowed" on the first strip show; size and position skipped | 11-28 | 08597bf | dSR::the_strip_is_moved_only_when_more_than_8_px_out_of_place, dSR::the_strip_is_never_shown_with_an_activating_flag (ShowWindowAsync with SW_SHOWNOACTIVATE) | partly manual (GPUI re-entrancy) |
| Stale strip position after a DPI or monitor change | 11-28 | 08597bf | dSR::the_strip_is_moved_only_when_more_than_8_px_out_of_place | automated |
| Finishing in Voxa's own window offered "Paste in Voxa" | 11-22 | 5aaf11d | cF::going_back_to_where_you_started_pastes_there_with_no_question; dM uses `is_own_window` (see Open items for the OS leg) | partly manual |
| macOS: the notification's Copy button took focus | 11-28 | aa0e718 | lint:focus-notify | automated |
| Esc registered all the time (Electron), breaking Esc in other apps | 11-22 | rewrite | dRu::esc_is_taken_only_while_recording_and_not_paused | automated |
| A test copy's window raised or answered the real Voxa window | 11-28 | 9acd6f3 | dW::a_test_copy_has_its_own_instance_names | automated |

### Desktop capture, clipboard and paste

| Bug in plain words | Found | Fix | Guarding test or lint | Kind |
|---|---|---|---|---|
| The dictation no longer stayed on the clipboard after pasting | 11-22 | c71f29d | dSt::settings_default_and_roundtrip (keep on clipboard is the default) | automated |
| Restoring the clipboard wiped images and files, raced at 250 ms | 11-28 | d580ecb | dW::the_clipboard_restore_keeps_text_files_and_images | partly manual (a live clipboard) |
| Dictations leaked into clipboard history and cloud sync | 11-28 | d580ecb | none automated beyond the format test; `mark_private` in dW | partly manual (Win+V) |
| Pasting into an admin window said "Pasted" | 11-28 | d580ecb | dRu::a_paste_into_an_admin_window_is_never_reported_as_pasted | automated |
| Choose the microphone in Settings, follow the default when it's gone | 11-22 | 8e0dcf3 | dA::the_chosen_mic_is_used_when_present_and_the_default_otherwise | automated |
| Mic stream dead after a Bluetooth reconnect, sleep or audio service restart; take lost as silence | 11-28 | 08597bf | dRu::a_mic_that_delivers_nothing_is_restarted_once_and_never_while_paused | partly manual (a real reconnect) |
| Mic marked missing until restart | 11-24 | 8adeb5b | cF::mic_blocked_is_caught_before_talking; the hotkey looks again (dM) | partly manual |
| Every mic failure shown as "access is off" | 11-28 | 260d324 | dA::only_refused_access_is_reported_as_privacy, dRu::mic_in_use_is_not_reported_as_privacy | automated |
| Shortcut dead if the window closed while a new one was picked | 11-24 | 8adeb5b | dRu::a_shortcut_left_paused_by_a_closed_picker_comes_back | automated |
| Full disk reported as "microphone blocked"; no way out | 11-28 | 08597bf, 6d552aa | cF::a_stopped_mic_or_full_disk_blocks_with_its_own_words, cF::a_full_disk_offers_to_free_up_space, dRu::every_transcribe_has_an_answer_and_trouble_is_job_scoped (MicError::Disk to CantSave) | automated |
| A warning and "checking sign-in" icon drew as empty boxes | 11-28 | 8553a49 | dH::the_icon_font_only_draws_its_own_code_points | automated |
| VS Code Antigravity terminal labelled Claude Code | 11-28 | 1a41be6 | dT::a_terminal_is_judged_by_its_own_title_not_by_claude_running_in_another_terminal, dT::claude_code_terminals_and_panel_are_still_named, dT::other_agent_clis_get_their_own_names | automated |
| "Unknown app" counts in History filters | 11-28 | investigated: only History imported from the old app has no app; new dictations are named by site, exe or title | dW::an_unknown_app_is_named_by_its_title_or_exe_never_unknown, desktop/app/src/import.rs::imports_old_history_keeping_text_recordings_and_shortcut | automated |
| Spoken "Newline" and "next para" not converted | 11-22 | 5c3b69b | cTT::spoken_new_lines_and_bullets | automated |
| Whisper invented "Thank you." on near-silent takes | 11-22 | 1e7b7db | core/voxa-store/src/take.rs::invented_phrases_only_count_when_quiet | automated |

### Desktop History, uploads and data

| Bug in plain words | Found | Fix | Guarding test or lint | Kind |
|---|---|---|---|---|
| Deleted or edited History entries came back | 11-24 | 8adeb5b | dRu::an_unchanged_job_is_not_written_to_history_again, cHD::an_edit_before_pasting_is_what_gets_pasted | automated |
| "Delete all" during a live take deleted its audio, then showed "Unreachable" | 11-24 | d1c85d2 | dSt::delete_all_spares_the_dictations_in_progress | automated |
| Unreadable History overwritten; later merge | 11-28 | 08597bf, 3929214 | dSt::unreadable_history_is_never_overwritten_and_merges_later | automated |
| Unreadable or damaged settings overwritten with defaults, pruning History to 30 days (`Settings::try_load`) | 11-28 | 3929214, 8553a49 | dSt::missing_settings_are_not_an_error, dSt::damaged_settings_are_an_error_and_kept_aside, dSt::unreadable_settings_are_never_saved_over | automated |
| Damaged history.json backup kept aside, never wiped | 11-28 | 32abed9, 2943ee0 | sR::a_damaged_history_is_copied_aside_once, sR::a_damaged_history_file_is_not_overwritten_by_the_next_edit | automated |
| Prune deleted unsent (Waiting or Failed) recordings | 11-24 | 8adeb5b | dSt::prune_keeps_unsent_recordings, dSt::prune_audio_keeps_text_and_unsent_recordings, sL::prune_drops_old_written_entries_but_never_unsent_ones | automated |
| Booting with the clock ahead deleted every dictation | 11-28 | 3929214 | dSt::a_clock_jump_at_startup_does_not_wipe_history, sRW::a_clock_jump_forward_does_not_delete_history | automated |
| Atomic writes raced on one fixed temp name | 11-28 | 5a587c7 | dSt::atomic_writes_use_their_own_temp_and_leave_none, lint:robust-atomic-write | automated |
| notify.json written with a bare write (torn file) | 11-28 | this audit | lint:robust-atomic-write (allow-list entry removed) | automated |
| Cancelling during "Writing it out" left orphan WAVs; a crash or power cut lost the take | 11-28 | 86bc6d8 | dRec::a_recording_with_no_history_entry_is_brought_back, dRec::a_take_with_nothing_recorded_is_dropped_with_its_empty_file, dRec::a_take_cut_off_by_a_forced_kill_is_kept_as_not_sent_and_plays, dRec::a_take_killed_while_being_written_out_again_keeps_its_text, dRec::finished_takes_are_left_alone | automated |
| A silent take was saved as "Couldn't reach" and kept forever | 11-28 | 3929214 | dRu::no_speech_is_never_filed_as_unreachable, dSt::no_speech_takes_are_not_unsent_and_are_pruned | automated |
| Long dictations could never send (20 s total timeout) | 11-24 | 8adeb5b, 66b3b8e | gL::upload_time_grows_with_the_recording, gR::upload_timeout_covers_a_long_take_on_the_slow_uplink_it_assumes | automated |
| Two uploads of one recording on slow internet when Retry was pressed mid-upload | 11-28 | 1903814 | cF::retry_while_the_old_upload_still_runs_never_uploads_twice, dU::one_recording_is_never_uploaded_twice_at_once | automated |
| The stale result of that upload flipped the entry back | 11-28 | 6410d8a | cF::a_duplicate_result_never_unpastes_a_dictation, cF::late_result_for_a_cancelled_job_does_not_paste | automated |
| A Timeout re-uploaded the whole take | 11-28 | d1c85d2, 86bc6d8 | dRu::a_timeout_is_never_uploaded_again_automatically, gR::audio_is_only_sent_again_when_nothing_went_out | automated |
| A flapping network re-uploaded every 5 s; all waiting takes uploaded at once | 11-28 | 86bc6d8 | dU::a_flapping_network_backs_off_and_then_stops_resending, dU::waits_grow_after_each_failure, dU::waiting_takes_are_uploaded_one_at_a_time, dU::the_take_on_the_strip_never_waits_behind_the_background_line, dU::the_user_asking_skips_the_wait | automated |
| Dropped uploads never retried; offline strip had no Retry | 11-22 | e3a5105 | cF::offline_strip_offers_retry_now_and_history_can_send_it, cF::going_offline_during_transcription_queues_the_job | automated |
| Retry then Cancel discarded a late transcript and re-uploaded it (flow Cancel restores the kept state) | 11-28 | 03dcb25 | cDR::h2_text_from_a_cancelled_retry_is_kept_so_the_recording_is_not_uploaded_again, cHD::text_from_a_cancelled_retry_is_kept_so_the_recording_is_not_uploaded_again, cF::cancelling_a_retry_keeps_the_recording | automated |
| `on_transcribed` acted outside Transcribing (a late answer hijacked a new take) | 11-28 | 6410d8a | cR::a_late_delivered_answer_does_not_hijack_a_new_recording, cF::a_late_failure_after_cancelling_a_retry_changes_nothing, cR::fuzz_duplicate_delivered_callbacks_never_break_the_flow | automated |
| A late answer fired a false "Couldn't send" toast | 11-28 | 86bc6d8 | dRu::a_late_answer_that_changes_nothing_sends_no_toast | automated |
| A mic problem on one take cancelled another take | 11-28 | 03dcb25 | cDR::h3_..., cHD::a_background_take_with_mic_trouble_never_cancels_the_take_being_recorded | automated |
| A mic error wedged a take in "Writing it out" | 11-28 | 86bc6d8 | cDR::h4_a_transcribe_the_shell_never_answers_wedges_the_job_until_restart, cHD::a_transcribe_answered_with_mic_trouble_never_wedges_the_job | automated |
| Signing in and coming back online in one check left waiting takes stuck | 11-28 | 03dcb25 | cDR::h1_..., cHD::signed_in_and_back_online_in_one_check_sends_every_waiting_take | automated |
| "Write it out again" did nothing for this run's dictations | 11-28 | 86bc6d8 | cHD::write_it_out_again_works_for_this_runs_dictations, dRu::send_now_and_write_it_out_again_are_routed_for_this_runs_dictations | automated |
| Finished takes counted as live (no Delete) | 11-28 | 86bc6d8 | cDR::h5_finished_jobs_are_not_in_progress, cHD::finished_jobs_are_not_in_progress | automated |
| Send now before the first check opened sign-in | 11-28 | 86bc6d8 | dRu::send_now_before_the_first_check_never_opens_sign_in | automated |
| A session check restored an older cookie; sign-out undone by late saves | 11-28 | 86bc6d8, 49cdac0 | dRu::a_session_check_never_restores_an_older_cookie, fL::sign_in_and_out_bump_the_generation_and_drop_waiting_cookies | automated |
| Signing in from the strip never cleared the signed-out marker | 11-28 | f8e79cb | dSi::a_saved_sign_in_clears_the_signed_out_marker | automated |
| Commands folder polled every 40 ms | 11-28 | 6ff7e99 | dL::the_commands_folder_is_not_read_every_tick | automated |
| Pause: stopping while paused counted the pause as talk time | 11-28 | 49cdac0 | dTk::stop_while_paused_leaves_the_pause_out, dTk::resume_adds_only_the_talking_after_it | automated |
| Pause and resume rules (one take, idempotent, hotkey stops, cancel discards, focus ignored) | 11-28 | 545dee5 | cF::pause_then_resume_keeps_one_take, cF::pause_and_resume_are_idempotent, cF::stop_while_paused_sends, cF::hotkey_while_paused_stops, cF::cancel_while_paused_discards_whole_take, cF::retry_while_paused_sends_in_the_background_and_keeps_the_pause, cF::readiness_changes_while_paused_keep_it_paused, cF::focus_while_paused_is_ignored, cF::paused_copy | automated |

### Desktop notifications and logs

| Bug in plain words | Found | Fix | Guarding test or lint | Kind |
|---|---|---|---|---|
| False "signed out" (Cloudflare, a rate-limit 403) | 11-28 | 22fc248, b044b44 | cF::cloudflare_block_never_asks_to_sign_in, gR::a_refusal_right_after_a_valid_token_is_not_a_sign_out, cE::a_wifi_sign_in_page_is_not_reported_as_cloudflare | automated |
| Toasts while looking at Voxa were dropped, not held back | 11-28 | aa0e718 | dN::a_toast_while_the_user_looks_at_voxa_is_held_back_not_dropped | automated |
| A dismissed toast bypassed the 6 h and 2 a day limits | 11-28 | aa0e718 | dN::a_dismissed_toast_counts_again_against_the_limits, cN::two_a_day_at_most, cN::dictations_at_most_every_six_hours | automated |
| "Update" popped a new banner | 11-28 | aa0e718 | cN::showing_updates_in_place | partly manual (the OS banner) |
| Notification times in the future silenced notifications | 11-28 | eb9b863 | cR::future_post_times_do_not_silence_notifications, cR::a_future_last_dictations_time_does_not_block_for_months | automated |
| Dictation text written into the desktop log | 11-28 | 49cdac0, 6ff7e99 | dRd::transcripts_never_reach_the_log, lint:privacy-log | automated |
| GPUI's window close errors ("window not found", "Invalid window handle") filled the log | 11-28 | 49cdac0 | dM::only_gpuis_window_close_messages_are_dropped | automated |

### Sign-in and test isolation

| Bug in plain words | Found | Fix | Guarding test or lint | Kind |
|---|---|---|---|---|
| ChatGPT sign-in expired 90 days after signing in (refreshed cookies never saved) | 11-28 | a2e17d4 | gL::refreshed_token_replaces_all_chunks_and_expired_cookies_go, gL::refreshed_sign_in_works_for_the_next_request | automated |
| Old refreshed cookies merged into a new sign-in | 11-28 | 66b3b8e | gL::refreshed_cookies_are_only_adopted_by_the_session_they_came_from, gL::a_new_sessions_cookies_never_merge_onto_an_old_slot, gL::clear_drops_waiting_cookies | automated |
| Concurrent session saves corrupted the file | 11-28 | 66b3b8e | gL::concurrent_saves_never_leave_a_broken_file | automated |
| Odd Max-Age or Expires deleted the token or overflowed | 11-28 | e405e9a | gR::a_huge_or_odd_max_age_does_not_delete_the_session_token, gR::absurd_expires_dates_are_rejected_not_overflowed, gL::a_past_expires_date_deletes_and_a_future_one_keeps, gL::a_live_value_wins_over_a_deletion_of_the_same_name | automated |
| A new `__cf_bm` alone counted as a refreshed sign-in | 11-28 | 66b3b8e | gL::only_a_new_cf_bm_is_not_worth_saving | automated |
| A test copy used the real Electron sign-in and a take uploaded | 11-28 | 481cece | lint:isolation-env | automated |
| A test copy imported the real Electron History | 11-28 | 4d1b256 | lint:isolation-env | automated |
| The sign-in window ignored VOXA_DATA | 11-28 | 195832f | lint:isolation-env (desktop/signin) | automated |
| Test hooks worked in a release build | 11-28 | 26559de | dM::test_hooks_need_a_test_folder_and_a_debug_build_or_voxa_test, lint:isolation-test-hook | automated |
| Finishing the tour in a test copy switched the real Start with Windows on | 11-22 | this audit (remote.rs `set_autostart`) | dRu::a_test_copy_never_changes_the_real_login_item | automated |
| The stand-in recorder (VOXA_FAKE) and the e2e harness ran on the real data | 11-22 | this audit | dM::the_stand_in_recorder_only_runs_in_a_test_copy | automated |

### Core and shared store

| Bug in plain words | Found | Fix | Guarding test or lint | Kind |
|---|---|---|---|---|
| Unicode text with Words or formatting crashed (byte slicing after a case change) | 11-28 | 624282f | cR::replace_phrases_never_panics_on_unicode_case_changes, cR::spoken_formatting_never_panics_on_unicode_case_changes, lint:robust-case-slice | automated |
| A 2xx reply without text was taken as "no speech" and the recording deleted | 11-28 | e405e9a | gR::a_reply_without_a_transcript_is_not_no_speech, fHR::an_empty_transcript_from_a_2xx_reply_does_not_delete_the_recording, fRW::an_unreadable_transcription_reply_keeps_the_recording | automated |
| Wi-Fi login pages reported as Cloudflare | 11-28 | b044b44 | cE::a_wifi_sign_in_page_is_not_reported_as_cloudflare, cE::failures_have_honest_reasons | automated |
| 401/403 right after a valid token caused a sign-in loop | 11-28 | 66b3b8e | gR::a_refusal_right_after_a_valid_token_is_not_a_sign_out | automated |
| Takes over about 13 minutes exceeded the upload limit | 11-28 | d0e8b63 | sA::a_long_take_is_sent_in_parts_under_the_upload_limit | automated |
| A cancelled retry of an offline take waited forever | 11-28 | 6410d8a | cR::a_cancelled_retry_of_an_offline_take_is_not_left_waiting_while_online | automated |
| A dismissed sign-in-to-send take was never sent | 11-28 | 6410d8a | cR::a_dismissed_sign_in_to_send_take_is_sent_after_signing_in | automated |
| A full disk left History empty; a cut-off save | 11-28 | ef21c62, 1d5a552, fc394fb | sR::an_empty_history_file_after_a_full_disk_falls_back_to_the_version_before, sR::a_save_cut_off_midway_falls_back_to_the_version_before | automated |
| History backup used a hard link (Android forbids it) | 11-28 | 1d5a552 | sR::a_save_cut_off_midway_falls_back_to_the_version_before (runs on Android's rename path too), aF::flowRoundTripInsertsAndSavesHistory | automated |
| Delete all left a copy of the deleted words | 11-28 | 1d5a552 | sR::delete_all_leaves_no_copy_of_the_deleted_words, fKR::a_deleted_dictation_leaves_no_copy_of_its_words | automated |
| Concurrent History writes mixed | 11-28 | 2943ee0 | sR::concurrent_atomic_writes_never_fail_or_mix, sR::queued_history_changes_apply_in_order_before_any_read | automated |
| Damaged history.json wiped History and every recording | 11-28 | 32abed9 | sRW::damaged_history_is_not_overwritten_by_the_next_take, sRW::zero_byte_history_is_not_overwritten_by_the_next_take, sR::a_damaged_history_turns_off_pruning_and_orphan_deletion, fRW::a_damaged_history_file_does_not_delete_unsent_recordings | automated |
| Colliding History ids | 11-28 | 6763aa0 | fKR::a_recovered_take_keeps_its_length (with unique ids), fT::dictation_round_trip_through_the_flow_saves_history | automated |
| A dictation filed under the start app after pasting elsewhere | 11-22 | 956ad45 | cF::switching_apps_asks_instead_of_pasting_into_the_wrong_chat, cF::moving_on_to_a_third_app_updates_the_question | automated |
| Expired session mid-dictation (the owner's WhatsApp case) | 11-22 | af6fa7e | cF::owner_whatsapp_case_expired_mid_dictation_resends_into_whatsapp_after_sign_in | automated |
| Reminders: night owls never reminded; future stamps blocked; read errors wiped state; non-atomic saves | 11-28 | 6aa5aaf | nR::a_night_owl_gets_a_reminder_inside_allowed_hours, nR::an_early_bird_gets_a_reminder_inside_allowed_hours, nR::a_reminder_stamped_far_in_the_future_does_not_block_reminders_until_that_date, nR::a_reminder_stamped_in_the_future_still_settles, nR::a_dictation_stamped_slightly_in_the_future_counts_as_recent, nR::a_transient_read_error_never_overwrites_the_saved_state, nR::a_leftover_temp_file_does_not_stop_saving, nR::a_reminder_that_cannot_be_saved_is_not_sent, nR::the_ignored_streak_never_wraps_back_to_zero, nR::coming_back_within_24h_counts_as_opened_even_with_a_later_dictation | automated |
| A damaged nudge.json turned reminders back on | 11-28 | 756f011 | nS::damaged_or_newer_files_keep_reminders_off, nE::a_damaged_file_keeps_reminders_off, fNu::a_damaged_state_file_keeps_reminders_off, nR::a_file_saying_on_with_one_bad_field_keeps_the_spacing_and_quiet_promise | automated |
| Seconds saved this month wrapped | 11-28 | 6aa5aaf | fNu::seconds_saved_this_month_stop_at_the_largest_number_instead_of_wrapping | automated |
| Greeting time slots: sunrise at 11 am, "Good afternoon" before noon | 11-28 | f3c3a7e | cG::time_slot_boundaries, cG::no_good_afternoon_before_noon_and_no_night_owl_before_ten | automated |
| First open at 10 pm showed Sunrise | 11-28 | f3c3a7e | cG::welcome_back_and_first_open | automated |
| Day names and "Happy New Year" flagged as Title Case by the lint | 11-28 | 4d6afa4 | lint fixture `Easy Sunday`, `Happy New Year` (Good lines in tools/lint/fixtures/bad/desktop/app/src/ui.rs, `--self-test`) | automated |
| Doodle tally always 7 marks; variants collided with the avatar | 11-28 | 36e3a20 | cD::the_tally_counts_for_real, cD::nothing_enters_the_avatar_corner_for_60_days, cD::hatching_stays_inside_its_shape | automated |
| Client creation could panic | 11-28 | 0b27184 | lint:robust-unwrap | automated |

### Android

| Bug in plain words | Found | Fix | Guarding test or lint | Kind |
|---|---|---|---|---|
| Tap to stop did nothing; only a hold stopped | 11-26 | 49d898e | aR::aTapWhileRecordingStopsTheTake, fAS::the_mic_key_uses_the_tested_rules | emulator |
| Crash every time Android closed the keyboard (engine torn down first; `destroyed` flag) | 11-26 | 49d898e | fAS::closing_the_keyboard_destroys_the_flow_only_after_the_system_is_done_with_it, fAS::work_arriving_after_the_keyboard_closed_does_nothing | partly manual (Android's teardown) |
| A hold whose press was taken away (look rebuild during a hold) left the mic on | 11-28 | 57e18d6 | aR::aHoldWhosePressIsTakenAwayStillStops | emulator |
| Recording during a call gave silence and "Didn't hear any speech" (`inCall` refusal) | 11-26 | 019b94f | aR::aCallRefusesTheMicAndSaysWhy | emulator; partly manual (a real call) |
| The mic taken mid-take (`onMicInterrupted`): keep what was said, say why when empty | 11-26 | 019b94f | aR::aMicTakenMidTakeKeepsWhatWasSaid | emulator |
| Recording stopped after 0.2 s: focus loss or "silenced" blip at start (grace, lasting confirmation) | 11-26 | 49d898e | aR::aFocusOrSilenceBlipAtTheStartIsNotAnInterruption | emulator; partly manual (the OnePlus phone) |
| A mic read error spun the CPU at 100 % | 11-28 | 16b57e2 | aR::aDeadRecorderEndsTheTakeInsteadOfSpinning | emulator |
| Storage full mid-take crashed the keyboard | 11-28 | 16b57e2 | aR::aRecordingThatCannotBeWrittenIsRefusedWithoutACrash, fT::a_take_cut_off_by_process_death_is_recovered_as_not_sent | emulator; partly manual (mid-take write failure) |
| Late mic levels moved the bars while paused | 11-28 | 519626d | fAS::mic_levels_after_a_pause_leave_the_bars_flat | automated |
| Launch animation almost never showed (`showLaunch` once per process) | 11-28 | 922717f | aR::theWrittenSplashPlaysOnEveryLauncherOpen | emulator |
| Keyboard look and Appearance changes never reached a live keyboard | 11-28 | 45e5361 | aK::everyChangeBumpsTheVersion, aR::anAppearanceChangeReachesALiveKeyboard | emulator |
| A rebuilt keyboard lost the navigation bar gap | 11-28 | 45e5361 | aR::aRebuiltPanelKeepsTheNavigationBarGap | emulator; partly manual (remembered inset on a real nav bar) |
| The cached profile picture survived sign out | 11-28 | 19e8512 | aR::signingOutForgetsTheProfilePicture | emulator |
| "There was a problem parsing the package" | 11-26 | ee9da25 | lint:android-signing | automated |
| A dictation went into another chat of the same app | 11-28 | dd9393a | aR::aFieldTheUserMovedToAndBackIsNotTheOldField, fT::moving_fields_asks_and_going_back_inserts | emulator |
| One attempt counter shared by every dictation; Cloudflare posted "couldn't send" | 11-28 | 57e18d6 | aR::eachDictationCountsItsOwnTriesAndCloudflareNeverPostsCouldntSend | emulator |
| False "signed out" notification | 11-28 | 0d393ad | aR::signedOutIsOnlyReportedWhenDefiniteAfterAGoodSession | emulator |
| Notices dropped, not deferred, while Voxa was on screen; lock screen privacy | 11-28 | 57e18d6 | aR::aNoticeWhileVoxaIsOnScreenIsHeldBackNotDropped | emulator |
| "Ready" posted for a cancelled take; keyboard Retry dead | 11-28 | d598397, 57e18d6 | fT::a_cancelled_take_has_no_entry_to_notify_about_and_only_live_takes_are_owned | automated; partly manual (Kotlin Retry button) |
| Background send looped or failed waiting takes on Retry | 11-28 | e7e0d86 | fN::a_retry_while_offline_never_marks_a_waiting_take_failed, fKR::h6_an_offline_answer_while_online_leaves_an_owned_entry_nobody_sends, fKR::a_waiting_take_is_never_stuck_behind_the_keyboard | automated |
| A dictation sent twice in the background | 11-28 | d598397 | fL::an_entry_can_only_be_claimed_once_at_a_time, fN::a_dictation_being_sent_is_never_sent_a_second_time, core/voxa-ffi/src/flow.rs::the_keyboard_never_uploads_an_entry_the_core_is_already_sending | automated |
| Incognito audio left on disk | 11-28 | d598397 | fT::incognito_audio_goes_when_the_take_ends_off_screen_and_after_a_process_death, fHR::an_incognito_take_that_can_still_be_retried_keeps_its_audio | automated |
| The keyboard rewrote History from stale state (deletes came back, edits undone, results reset) | 11-28 | c889f63 | fKR::h1_..., fKR::h2_a_deleted_dictation_stays_deleted, fKR::h2b_clear_history_stays_cleared, fKR::h3_..., fHR::a_later_keyboard_event_does_not_overwrite_a_background_send_result, fHR::a_deleted_entry_stays_deleted_after_the_next_keyboard_event, fHR::an_edited_entry_keeps_its_edit_after_the_next_keyboard_event | automated |
| A keyboard switch mid-upload dropped the result and uploaded again | 11-28 | dd9393a | fKR::h5_a_keyboard_restart_mid_upload_does_not_start_a_second_upload, fKR::a_keyboard_switch_mid_upload_keeps_the_result_in_history | automated |
| A recovered take had length 0 | 11-28 | 1063011 | fKR::a_recovered_take_keeps_its_length | automated |
| Contrast: light secondary text, "Pasted in", Teal and Rose mics, averaged photo grid | 11-28 | dd5ad55, 11145d8, 1a01de1, f30865b | aC::voxaLookSecondaryTextAndPastedTitleReadInLightAndDark, aC::everyMicColourGivesItsIconAtLeast4_5, aC::theOldTealAndRoseAreReadAsTheNewOnes, aC::aSmallBrightDetailInADarkCellIsNotAveragedAway, lint:contrast | emulator and automated |
| A broken photo crashed the keyboard | 11-28 | 45e5361 | aK::aBrokenPhotoFallsBackToVoxaWithoutCrashing | emulator |
| Blank ChatGPT login WebView and sign-in loop (old app) | 11-22 | f4d947e | cF::cloudflare_block_never_asks_to_sign_in (the shared rule); the old app was removed | partly manual |
| History edit, delete and clear on the main thread | 11-28 | 5d95b8b | sR::queued_history_changes_apply_in_order_before_any_read | partly manual (main thread) |

## Decisions

| Decision | Source | Guarding lint or test |
|---|---|---|
| WCAG AA everywhere, AAA for body and secondary text, light and dark (desktop, app, keyboard) | feedback_wcag_contrast, guardrails 1 | lint:contrast, dTh::text_and_components_meet_their_contrast_targets, dTh::letters_on_brand_colours_are_readable, aC (all), aK::everyPresetReads |
| Brand sepia #7A3F22 and the 1.0 palette | feedback_wcag_contrast, e8bb101 | lint:contrast (a light accent other than #7A3F22 fails), aK::theVoxaLookIsTheV6PaletteWithThe10Sepia |
| No emojis in the UI (emoji panel exempt) | feedback_no_emojis | lint:copy-emoji, cN::copy_has_no_dashes_or_emojis, nM::copy_has_no_emojis_dashes_or_shouting |
| No dashes in copy | guardrails 2 | lint:copy-dash |
| Sentence case | guardrails 2 | lint:copy-case (with the day-name fixture) |
| Culture-neutral: occasions only New Year, birthday, own milestones, anniversary | feedback_culture_neutral | lint:copy-occasion, cG::priority_birthday_then_new_year_then_milestone_then_time, cG::one_year_with_voxa |
| Never steal focus | guardrails 4 | lint:focus-steal, lint:focus-notify, dSR::the_strip_is_never_shown_with_an_activating_flag, dM::the_strip_window_opens_without_focus |
| Never log cookies, tokens or transcripts; notifications never carry transcript text | guardrails 3 | lint:privacy-log, lint:privacy-notify, dRd::transcripts_never_reach_the_log, fT::diagnostics_log_events_but_never_the_transcript |
| Android tiny: APK budget, dependency allow-list | project_android_small_lean | lint:android-size (with an APK), lint:android-deps |
| Android: no WorkManager, JobScheduler only | project_android_small_lean | lint:android-workmanager |
| Android APK signed v1, v2 and v3 | 11-26 parse error | lint:android-signing |
| Android GitHub releases `--latest=false` | guardrails 7 | lint:release-latest |
| History "Your apps" cap stays at 8 | owner, 11-28 | aR::yourAppsStaysCappedAtEight |
| Greeting on one line when it fits | owner, 11-28 | aR::theGreetingIsOnOneLineWhenItFits |
| Time slots: Sunrise 5-8, Morning 8-11:30, Midday 11:30-16, Evening 16-19, Night 19-5; Night owl only from 22:00 | doodle review, owner | cG::time_slot_boundaries, cG::no_good_afternoon_before_noon_and_no_night_owl_before_ten |
| Splash variant B timing (2.42 s, slow start) | owner, 11-28 | cS::the_beats_land_on_time, cS::the_write_starts_slowly_then_finds_its_pace, cS::easing_matches_its_ends |
| Doodles decorative, finished by 1400 ms, never over the avatar corner, deterministic daily variation, real moon | doodle review | cD::every_doodle_draws_and_finishes_by_1400_ms, cD::nothing_enters_the_avatar_corner_for_60_days, cD::variations_are_deterministic_and_do_vary, cG::moon_phase_matches_known_dates, fG::greeting_plays_once_per_doodle_per_day |
| Reminders: one per 3 days | docs/reminders.md | nRu::at_most_one_every_3_days |
| Reminders: none 21:00 to 09:00 (the midnight birthday wish is the owner-approved exception, to be added with its own test) | docs/reminders.md | nRu::never_between_21_and_9 |
| Reminders: none within 48 h of use | docs/reminders.md | nRu::never_within_48_hours_of_a_dictation |
| Reminders: stop after 2 ignored | docs/reminders.md | nRu::stops_after_two_ignored_until_the_user_dictates, nE::two_ignored_then_silence_until_the_user_dictates |
| Reminders: off switch; damaged state keeps them off | docs/reminders.md | nRu::off_switch, fNu::reminders_follow_history_and_the_switch, nS::damaged_or_newer_files_keep_reminders_off |
| Reminders: usual-hour rule is the default, network an experiment | owner, 118b497 | nE::the_default_policy_is_the_usual_hour, nE::rules_win_over_an_eager_model |
| Notifications: one per kind updated in place, at most 2 a day, dictations every 6 h, quiet hours silent | docs/notifications.md | cN::showing_updates_in_place, cN::two_a_day_at_most, cN::dictations_at_most_every_six_hours, cN::quiet_hours |
| Notifications: never while the user looks at Voxa; held back, never dropped | docs/notifications.md | dN::a_toast_while_the_user_looks_at_voxa_is_held_back_not_dropped, aR::aNoticeWhileVoxaIsOnScreenIsHeldBackNotDropped |
| Notifications: "signed out" only when definite, after a good session, confirmed | docs/notifications.md | aR::signedOutIsOnlyReportedWhenDefiniteAfterAGoodSession, gR::a_refusal_right_after_a_valid_token_is_not_a_sign_out |
| Notifications: never for Cloudflare blocking; lock screen private | docs/notifications.md | aR::eachDictationCountsItsOwnTriesAndCloudflareNeverPostsCouldntSend, aR::aNoticeWhileVoxaIsOnScreenIsHeldBackNotDropped (VISIBILITY_PRIVATE) |
| Birthday: day and month only, stored on the phone | owner, 11-28 | cG::birthdays_are_day_and_month_only, fG::birthday_wins_and_is_optional, aR::theBirthdayIsDayAndMonthOnlyAndStaysOnThePhone |
| Test copies never touch real data, the real sign-in or the real login item | owner, 11-28 incident | lint:isolation-env, lint:isolation-test-hook, dRu::a_test_copy_never_changes_the_real_login_item, dM::the_stand_in_recorder_only_runs_in_a_test_copy, dW::a_test_copy_has_its_own_instance_names |
| Dictation stays on the clipboard by default | owner, 11-22 | dSt::settings_default_and_roundtrip |
| Esc is Voxa's only while recording | rebuild flow audit | dRu::esc_is_taken_only_while_recording_and_not_paused |
| During a call, refuse to record and say why | owner, 11-26 | aR::aCallRefusesTheMicAndSaysWhy |
| Shortcuts need Ctrl or Alt and a real key | rebuild flow audit | dH::shortcuts_need_ctrl_or_alt_and_a_real_key |
| Appearance follows the system unless set | owner, 11-28 | dSt::appearance_round_trips_and_unknown_values_follow_the_system, dTh::the_setting_overrides_the_system |
| Password fields blocked; incognito keeps nothing | Android spec | aF::passwordFieldsAreBlocked, fT::password_and_incognito_fields, fT::private_fields_keep_nothing |
| Silent takes never sent; no speech offers no retry | flow audit | fT::silent_recordings_are_never_sent, cF::no_speech_offers_no_retry_and_closes_itself |
| No byte slicing after a case change; no unwrap in core; atomic unique-temp writes | bug hunt, guardrails 8 | lint:robust-case-slice, lint:robust-unwrap, lint:robust-atomic-write |
| Every bug gets a test and every decision a lint | owner, 11-28 | this file, `--self-test` (every rule has a fixture) |
| Android releases are built in CI on an android-v tag, a prerelease until promoted; the key only in the protected release environment, never with K1 in one job, never echoed, deleted after | owner, 11-28 | lint:release-android-ci, lint:release-latest (android-release.yml), lint:release-feed-guard; the first real tag run is manual (runbook D16) |
| HTML mockup first for a new UI direction; never name the owner's personal apps in the repo; a Mac change never alters Windows | owner | process rules: reviewed by hand (no automated check is possible without naming the private apps) |

## Voxa 1.0.0 final release review (11-28)

Findings of the release review (5 blockers, 22 shoulds, the cheap nits) and what now guards them.
`vU` is core/voxa-update/src, `dUp` desktop/app/src/update.rs, `aUS` the Android instrumented
UpdateSafetyTest.

| Finding | Guarding test or lint | Kind |
|---|---|---|
| B1 a hand-built 1.0.0 with placeholder update keys could never update | vU keys::tests::guard::a_release_build_with_placeholder_keys_fails_unless_the_dev_flag_is_set, lint:release-placeholder-guard (build.rs, Gradle releaseRefusal, voxa.iss #error), dUp::a_build_with_placeholder_keys_says_updates_are_off, aUS::a_build_with_placeholder_keys_says_updates_are_off_and_never_checks | automated; ISCC and assembleRelease refusals run by hand |
| B2 README Android link was a 404 | lint:release-readme-links | automated |
| B3/B4 README said uninstall; Android signing undecided (decision: one release key from 1.0.0, test builds uninstalled once, no lineage) | lint:release-android-signing (forbidden test-build signer e648aa43..., pinned release-cert.sha256 with --release, no lineage step), Gradle releaseRefusal + verifyReleaseSigner | automated; the release key itself is an owner step |
| B5 the updater never ran on real hardware | docs/release-runbook.md REAL-HARDWARE RELEASE GATE, lint:release-runbook | manual gate |
| S1 android-feed (now android-release.yml) dispatched from main is refused by the release environment | lint:release-feed-guard (refs/tags/$TAG check) | automated |
| S2 a rollback's relaunch retried at once and burnt the 2-attempt cap | vU policy::tests::a_failed_install_waits_hours_before_the_second_attempt, dUp::a_start_by_setup_never_installs_again_at_once | automated |
| S3 auto-update treated as on when settings.json was unreadable | dUp::auto_update_counts_as_off_when_settings_cannot_be_read | automated |
| S4 "Restart and update" left a frozen window that Setup killed after 20 s | dUp::restart_and_update_closes_the_window_first_and_reopens_it | partly manual (the window closing on real hardware) |
| S5 no kill switch; a staged file installed after a paused feed | vU policy::tests::a_paused_or_changed_feed_withdraws_the_staged_file_before_install, vU fetch::tests::a_paused_feed_stops_a_staged_install_at_the_last_check, ffi update::tests::no_network_at_the_last_look_never_withdraws_a_verified_file, lint:release-feed-guard (feed-pause.yml) | automated; the drill is manual (runbook D17) |
| S6 feed publishing could move the version backwards | vU policy::tests::the_feed_version_only_moves_forward_when_publishing, lint:release-feed-guard (guard-forward) | automated |
| S7 a version that crashes at start could never update | dUp::a_version_that_dies_at_start_twice_looks_for_a_fixed_one_first | partly manual (a real crashing build) |
| S8 a renamed GitHub asset host would break every client | vU lib::tests::a_redirect_to_a_renamed_github_asset_host_still_works | automated |
| S9 Android re-downloaded a refused APK every day | aUS::a_download_the_phone_refused_is_never_fetched_again | emulator |
| S10 key rotation only on Android 13+ | obsolete: no rotation (owner decision), lint:release-android-signing | automated |
| S11 Android ignored the manual-only flag | aUS::a_manual_only_offer_is_never_downloaded_and_points_to_the_release_page | emulator |
| S12 words.json reset to empty when locked or damaged | dSt::a_damaged_word_list_is_kept_and_never_overwritten_by_an_edit | automated |
| S13 0.4.x History compatibility unproven (Status enum is unchanged since v0.4.0) | dSt::a_0_4_x_history_file_loads_with_every_entry | automated |
| S14 instrumented tests could wipe a real phone | Gradle refuses connected*AndroidTest unless ANDROID_SERIAL=emulator-* | automated |
| S15 full window titles in voxa-desktop.log | lint:privacy-log (describe() never reads the title) | automated |
| S16 the strip showed and could activate at every start and after an update | dM::the_strip_never_flashes_or_activates_at_start_or_after_an_update | partly manual (focus on real hardware, runbook D2) |
| S17 known main-flow bugs (paste loss, "Pasted" without a text box) | runbook D8 | manual, still open below |
| S18 no ordered release runbook | lint:release-runbook | automated |
| S19 macOS always signed ad hoc | package.sh uses VOXA_MAC_SIGN_IDENTITY and runs check-signature.sh | manual (no Mac CI run yet, runbook D18) |
| S20 README Android install guidance | lint:release-readme-links (names), README text | partly manual |
| S21/S22 docs and CI disagreed; rotation steps | --release lint on tags in desktop.yml and guardrails.yml; update-keys.md rewritten | automated (the lint run) |
| N1 a damaged update.json silently reset | dUp::a_damaged_update_state_is_kept_aside_not_silently_reset | automated |
| N2 a stale offer survived a manual install | dUp::an_offer_for_the_installed_version_or_older_is_dropped | automated |
| N3 cargo test wrote into the real desktop log | desktop/app/src/common.rs log_path_tests::cargo_test_never_writes_the_real_desktop_log | automated |
| N8 uninstall left updates and install logs | dUp::install_logs_and_broken_state_files_are_capped | automated |
| N9 versionCode collision at minor or patch 100 | vU version::tests::minor_or_patch_above_99_is_rejected_so_version_codes_never_collide | automated |
| N10 asset name matched a version substring | vU feed::tests::an_asset_name_must_carry_the_exact_version_not_a_substring | automated |
| N12 the update notification was public on the lock screen | Android UpdateNotificationTest::a4_has_the_title_and_the_public_lock_screen_text (VISIBILITY_PRIVATE) | emulator |
| N15/N16 k.pem left on failure; no empty-secret check on desktop | lint:release-feed-guard | automated |
| N20 greeting ledger written with a bare fs::write | desktop/app/src/greet.rs tests::the_greeting_ledger_is_written_atomically | automated |
| N4 ignored manual tests read the real desktop session | core/voxa-chatgpt lib.rs manual_gate_tests::manual_tests_never_read_the_real_session_without_the_env_flag (VOXA_MANUAL_REAL_SESSION=1) | automated |

## Open items found by this audit

Not fixed; each needs a fix with its test (and was reported to the owner):

- Pasting where there is no text box still says "Pasted" (only the admin-window case is caught).
- Suspected in the hunts and never confirmed or fixed: the Android diagnostics sink calling into
  Kotlin while locks are held; the usual reminder hour shifting by one across a DST change; week
  stats drifting with DST or travel; 429 retries ignoring Retry-After; a Bluetooth headset staying
  in call mode while paused; the macOS paste key code on Dvorak; Android transient audio focus and
  Bluetooth routing.
- Open in verification reports: Android Home showing "Connected" while offline; a paste lost in
  about one in five spike runs; a faint dark panel behind the strip; Android memory above target.
- macOS fixes written but never run on a Mac (sign-in window, Accessibility permission, IME).
- Android instrumented tests are not in CI (no emulator there).
