import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import type {
  DictationNotice,
  DictationStatusEvent,
  StreamPhase,
  StreamPhaseEvent,
  StreamTextEvent,
  StreamWorkKind,
} from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import type { ModelStateEvent } from "@/lib/types/events";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { keyLabel } from "@/lib/kusemaKey";
import type { KusemaKey } from "@/bindings";

type OverlayState =
  | "recording"
  | "streaming"
  | "transcribing"
  | "processing"
  | "speaking"
  | "dictating"
  // Kusema voice editing: the editing hotkey is held, or a message about it.
  | "editing"
  | "notice";

/** Kusema voice editing: a message for the overlay. */
type EditNotice = {
  key: string | null;
  name: string | null;
  message: string;
  in_dictation: boolean;
};

/** How long a voice editing message replaces continuous dictation's status. */
const EDIT_NOTICE_MS = 5000;

/** Kusema: how long the "done" look shows as the pill fades (ov-fade is 200ms). */
const LEAVING_MS = 260;

/** Kusema: which mode the pill is listening for (its icon). */
type PillMode = "dictation" | "ask" | "ask_screen" | "edit" | "continuous";

/** Kusema styles per app: the style about to change the text. */
type AppStyleEvent = { id: string; name: string | null };

/** Continuous dictation notices that mean something went wrong. */
const ERROR_NOTICES: DictationNotice[] = [
  "microphone_lost",
  "cant_keep_up",
  "transcription_failed",
  "paste_failed",
];

/** The pill's look, from what's happening. */
type Look =
  | "listening"
  | "thinking"
  | "speaking"
  | "paused"
  | "error"
  | "notice"
  | "done";

const PATHS: Record<PillMode, string> = {
  // Microphone.
  dictation:
    "M8 2.5a2 2 0 0 1 2 2v3.5a2 2 0 0 1-4 0V4.5a2 2 0 0 1 2-2ZM4.5 7.5a3.5 3.5 0 0 0 7 0M8 11v2.5",
  // Microphone with a loop: hands-free.
  continuous:
    "M8 2.5a2 2 0 0 1 2 2v3.5a2 2 0 0 1-4 0V4.5a2 2 0 0 1 2-2ZM4.5 7.5a3.5 3.5 0 0 0 7 0M8 11v2.5M2.5 5.5v4M13.5 5.5v4",
  // Speech bubble.
  ask: "M3 4.5a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H7.5L4.5 13v-2.5H5a2 2 0 0 1-2-2Z",
  // Screen.
  ask_screen: "M2.5 3.5h11v7h-11ZM6 13.5h4M8 10.5v3",
  // Pencil.
  edit: "M10.5 3 13 5.5 6 12.5H3.5V10Z",
};

const ICONS = {
  pause: "M5.5 3.5v9M10.5 3.5v9",
  warn: "M8 2.5 14 13H2ZM8 6.5v3M8 11.2v.1",
  info: "M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12ZM8 7.5v3.5M8 5.2v.1",
  speak:
    "M3 6.5h2.5L9 3.5v9l-3.5-3H3ZM11 6a2.5 2.5 0 0 1 0 4M12.5 4.5a4.5 4.5 0 0 1 0 7",
};

const PillIcon: React.FC<{ d: string; className?: string }> = ({
  d,
  className = "",
}) => (
  <svg className={`sicon ${className}`} viewBox="0 0 16 16" aria-hidden="true">
    <path
      d={d}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const DICTATION_NOTICES: Record<DictationNotice, string> = {
  nothing_to_scratch: "overlay.dictation.nothingToScratch",
  scratch_window_changed: "overlay.dictation.scratchWindowChanged",
  scratch_other_input: "overlay.dictation.scratchOtherInput",
  saved_to_history: "overlay.dictation.savedToHistory",
  stopped_silence: "overlay.dictation.stoppedSilence",
  microphone_lost: "overlay.dictation.microphoneLost",
  cant_keep_up: "overlay.dictation.cantKeepUp",
  transcription_failed: "overlay.dictation.transcriptionFailed",
  paste_failed: "overlay.dictation.pasteFailed",
};

// Number of reactive bars in the waveform (the simple, smoothed style shared by
// every overlay form). Mic levels arrive as 16 FFT buckets; we take the first N.
const WAVE_BARS = 9;

// Only call out a model load in the Live preview once it has run this long.
// Warm loads finish in well under this (~0.2s on Apple Silicon, ~1.5s on a
// Windows CPU backend), so the common case never flashes a loading notice while
// the user is speaking; cold loads can take 40s+.
const SLOW_MODEL_LOAD_MS = 2000;

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  // `Stream::play()` returning does not mean hardware callbacks are flowing.
  // Stay visually in an arming state until the backend processes the first
  // actual microphone sample chunk.
  const [captureReady, setCaptureReady] = useState(false);
  // Recording starts while the model loads in the background. Say so, otherwise
  // a cold start looks like an empty Live preview that ignores speech, then a
  // hung "Transcribing..." spinner. Once recording stops, any in-flight load is
  // the wait, so the working label reflects it immediately; the Live notice
  // waits for the load to be slow so fast loads don't flash it mid-speech.
  const [modelLoading, setModelLoading] = useState(false);
  const [modelLoadSlow, setModelLoadSlow] = useState(false);
  const modelLoadTimerRef = useRef<ReturnType<typeof setTimeout>>();
  // Latched per Live session: once the loading notice has opened the panel, keep
  // it open after the load so it doesn't collapse and reopen when text arrives.
  const [loadNoticeShown, setLoadNoticeShown] = useState(false);
  const [levels, setLevels] = useState<number[]>(Array(WAVE_BARS).fill(0));
  const [streamText, setStreamText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = useState<StreamPhase>("listening");
  const [workKind, setWorkKind] = useState<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = useState(0);
  const [dictation, setDictation] = useState<DictationStatusEvent | null>(null);
  const [now, setNow] = useState(Date.now());
  const [privileged, setPrivileged] = useState(false);
  // Kusema voice editing: the message shown, and until when in dictation.
  const [editNotice, setEditNotice] = useState("");
  const [editNoticeUntil, setEditNoticeUntil] = useState(0);
  // Kusema: the mode being listened for, the app style in use, and the
  // moment the pill is leaving (its "done" flourish).
  const [mode, setMode] = useState<PillMode>("dictation");
  const [appStyle, setAppStyle] = useState<AppStyleEvent | null>(null);
  // Kusema: where "Kusema, …" went (ask, ask_screen, edit), for the label.
  const [route, setRoute] = useState<string | null>(null);
  // Kusema key: Space was tapped while holding, so this take is a question.
  const [assistantTake, setAssistantTake] = useState(false);
  const kusemaKeyRef = useRef<KusemaKey | undefined>(undefined);
  const [leaving, setLeaving] = useState(false);
  const leavingTimerRef = useRef<ReturnType<typeof setTimeout>>();
  // Bumped on each new streaming session so the Live card remounts fresh (replays
  // the pop-in, and never animates in from the previous panel's open size).
  const [session, setSession] = useState(0);
  // Overlay placement (top vs bottom of the screen). The Live panel grows downward
  // from a top overlay (oldest line under the pill) and upward from a bottom one.
  const [position, setPosition] = useState<"top" | "bottom">("bottom");
  // True once live text overflows the cap. A top overlay fades its top edge only
  // while overflowing, so the resting first line stays crisp flush under the pill.
  const [overflowing, setOverflowing] = useState(false);

  const smoothedLevelsRef = useRef<number[]>(Array(16).fill(0));
  // Live-text scroll-back: the text region "sticks" to the newest line while the
  // user is at the bottom; if they scroll up to read history, auto-follow pauses
  // until they scroll back down.
  const capRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  const direction = getLanguageDirection(i18n.language);

  useEffect(() => {
    const setupEventListeners = async () => {
      const unlistenShow = await listen("show-overlay", async (event) => {
        const overlayState = event.payload as OverlayState;
        // Reset synchronously before settings I/O. A fast microphone can emit
        // recording-ready while the awaits below are in flight; resetting after
        // them would overwrite that event and leave the overlay stuck arming.
        clearTimeout(leavingTimerRef.current);
        setLeaving(false);
        if (overlayState !== "processing" && overlayState !== "notice") {
          setAppStyle(null);
          setRoute(null);
          setAssistantTake(false);
        }
        if (overlayState === "dictating") {
          setDictation(null);
          smoothedLevelsRef.current = Array(16).fill(0);
          setLevels(Array(WAVE_BARS).fill(0));
        }
        if (
          overlayState === "recording" ||
          overlayState === "streaming" ||
          overlayState === "editing"
        ) {
          setCaptureReady(false);
          smoothedLevelsRef.current = Array(16).fill(0);
          setLevels(Array(WAVE_BARS).fill(0));
          setStreamText({ committed: "", tentative: "" });
        }

        await syncLanguageFromSettings();
        // The Live panel flows downward from a top overlay and upward from a
        // bottom one; read the placement so the layout can flip to match.
        try {
          const settings = await commands.getAppSettings();
          if (settings.status === "ok") {
            setPosition(
              settings.data.overlay_position === "top" ? "top" : "bottom",
            );
            setPrivileged(settings.data.privileged_mode ?? false);
            kusemaKeyRef.current = settings.data.kusema_key;
          }
        } catch {
          // Keep the previous/default placement if settings can't be read.
        }
        setState(overlayState);
        if (overlayState === "streaming") {
          setPhase("listening");
          setWorkKind("transcribing");
          setElapsed(0);
          setLoadNoticeShown(false);
          setSession((s) => s + 1); // remount the card fresh for this session
        }
        setIsVisible(true);
      });

      const unlistenHide = await listen("hide-overlay", () => {
        // The "done" look plays while the pill fades, then it's gone, so a
        // later show never flashes this content.
        setLeaving(true);
        clearTimeout(leavingTimerRef.current);
        leavingTimerRef.current = setTimeout(
          () => setLeaving(false),
          LEAVING_MS,
        );
        setIsVisible(false);
        setCaptureReady(false);
      });

      const unlistenMode = await listen<PillMode>("overlay-mode", (event) =>
        setMode(event.payload),
      );

      const unlistenStyle = await listen<AppStyleEvent>("app-style", (event) =>
        setAppStyle(event.payload),
      );

      const unlistenRoute = await listen<string>("overlay-route", (event) =>
        setRoute(event.payload),
      );

      const unlistenAssistantTake = await listen("overlay-assistant-take", () =>
        setAssistantTake(true),
      );

      const unlistenEditNotice = await listen<EditNotice>(
        "voice-edit-notice",
        (event) => {
          const notice = event.payload;
          // The hint names the Kusema key in the user's language.
          const name =
            notice.key === "overlay.triggerHint"
              ? keyLabel(kusemaKeyRef.current, (k) => i18n.t(k))
              : (notice.name ?? "");
          setEditNotice(
            notice.key ? i18n.t(notice.key, { name }) : notice.message,
          );
          setEditNoticeUntil(
            notice.in_dictation ? Date.now() + EDIT_NOTICE_MS : 0,
          );
        },
      );

      const unlistenReady = await listen("recording-ready", () => {
        setElapsed(0);
        setCaptureReady(true);
      });

      const unlistenPrivileged = await listen<boolean>(
        "privileged-changed",
        (event) => setPrivileged(event.payload),
      );

      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        const newLevels = event.payload as number[];
        // Exponential smoothing across the 16 buckets, then take the first N
        // bars for the shared waveform.
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3;
        });
        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, WAVE_BARS));
      });

      const unlistenStream = await events.streamTextEvent.listen((event) => {
        setStreamText(event.payload);
      });

      const unlistenDictation = await events.dictationStatusEvent.listen(
        (event) => {
          setDictation(event.payload);
          setNow(Date.now());
        },
      );

      const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
        const payload: StreamPhaseEvent = event.payload;
        setPhase(payload.phase);
        if (payload.kind) setWorkKind(payload.kind);
      });

      // The backend ends every `loading_started` with exactly one of completed
      // or failed, so only those end a load. Other events (e.g. `unloaded` from
      // the idle watcher) aren't ordered against an in-flight load and must not
      // clear it.
      const unlistenModel = await listen<ModelStateEvent>(
        "model-state-changed",
        (event) => {
          const type = event.payload.event_type;
          if (type === "loading_started") {
            clearTimeout(modelLoadTimerRef.current);
            setModelLoading(true);
            setModelLoadSlow(false);
            modelLoadTimerRef.current = setTimeout(
              () => setModelLoadSlow(true),
              SLOW_MODEL_LOAD_MS,
            );
          } else if (
            type === "loading_completed" ||
            type === "loading_failed"
          ) {
            clearTimeout(modelLoadTimerRef.current);
            setModelLoading(false);
            setModelLoadSlow(false);
          }
        },
      );

      return () => {
        unlistenShow();
        unlistenHide();
        unlistenMode();
        unlistenStyle();
        unlistenRoute();
        unlistenAssistantTake();
        unlistenReady();
        unlistenEditNotice();
        unlistenLevel();
        unlistenStream();
        unlistenPrivileged();
        unlistenDictation();
        unlistenPhase();
        unlistenModel();
        clearTimeout(modelLoadTimerRef.current);
        clearTimeout(leavingTimerRef.current);
      };
    };

    setupEventListeners();
  }, []);

  // Elapsed capture timer starts only once microphone samples are flowing.
  useEffect(() => {
    if (state !== "streaming" || !isVisible || !captureReady) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [state, isVisible, captureReady]);

  // The dictation timer counts from the session's start, paused or not.
  useEffect(() => {
    if (state !== "dictating" || !isVisible) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [state, isVisible]);

  // Stick to the bottom as text streams in — but only while pinned, so a user who
  // has scrolled up to read history isn't yanked back down by the next chunk.
  useLayoutEffect(() => {
    const el = capRef.current;
    if (!el) return;
    // Fade the top edge only once text actually overflows the cap.
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
    if (pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [streamText]);

  // `session` re-runs this when a new Live session starts mid-load (e.g. a
  // retry during a cold load), since the reset doesn't change the other deps.
  useEffect(() => {
    if (modelLoadSlow && state === "streaming") setLoadNoticeShown(true);
  }, [modelLoadSlow, state, session]);

  // Each fresh streaming session starts pinned to the bottom, fade cleared.
  useEffect(() => {
    pinnedRef.current = true;
    setOverflowing(false);
  }, [session]);

  // Kusema: kept mounted for a moment after hiding, for the "done" look.
  if (!isVisible && !leaving) return null;

  // Re-pin when the user is within ~a line of the bottom; unpin otherwise.
  const handleStreamScroll = () => {
    const el = capRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  const fmtTime = (s: number) =>
    `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  const transcribingLabel = modelLoading
    ? t("overlay.loadingModel")
    : t("overlay.transcribing");

  // Kusema: the voice level drives the glow; the style, when one is shaping
  // the text, is named in the working label.
  const level = Math.min(
    1,
    (levels.reduce((sum, v) => sum + v, 0) / Math.max(1, levels.length)) * 1.6,
  );
  const styleName = appStyle
    ? (appStyle.name ??
      t(`settings.appStyles.builtIn.${appStyle.id}.name`, {
        defaultValue: appStyle.id,
      }))
    : null;
  const styledLabel = (label: string) =>
    route
      ? t(`overlay.route.${route}`)
      : styleName
        ? t("overlay.withStyle", { name: styleName })
        : label;
  const stageStyle = { "--lvl": level.toFixed(3) } as React.CSSProperties;
  const glow = <span className="sglow" aria-hidden="true" />;
  const modeIcon = (pillMode: PillMode) => (
    <PillIcon
      d={PATHS[pillMode]}
      className={captureReady || pillMode === "continuous" ? "ready" : "arming"}
    />
  );

  // ---- Shared building blocks (one visual language for every overlay form) ----
  const waveform = (
    <div className={`swave ${captureReady ? "ready" : "arming"}`}>
      {levels.map((v, i) => (
        <i
          key={i}
          style={{
            height: `${Math.max(3, Math.min(18, 3 + Math.pow(v, 0.7) * 15))}px`,
          }}
        />
      ))}
    </div>
  );

  const privilegedChip = privileged && (
    <span className="pchip">{t("overlay.privileged")}</span>
  );

  const cancelBtn = (
    <button
      className="sx"
      aria-label="cancel"
      onClick={() => commands.cancelOperation()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );

  // dot (left) | waveform (center) | timer + cancel (right) — same structure for
  // pill & panel, so the Live morph is a pure width change.
  const listeningRow = (showTimer: boolean, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        {modeIcon(state === "editing" ? "edit" : mode)}
        {state === "editing" && (
          <span className="pchip">{t("overlay.editing")}</span>
        )}
        {assistantTake && state !== "editing" && (
          <span className="pchip">{t("overlay.assistantTake")}</span>
        )}
        {privilegedChip}
      </div>
      {waveform}
      <div className="sbase-r">
        {showTimer && <span className="stimer">{fmtTime(elapsed)}</span>}
        {showCancel && cancelBtn}
      </div>
    </div>
  );

  // spinner (left) | label (center) | cancel (right) — same 3-zone grid as the
  // listening row, so the label is centered.
  const workingRow = (label: string, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        {state === "speaking" ? (
          <PillIcon d={ICONS.speak} className="ready speaking" />
        ) : (
          <span className="sspinner" />
        )}
        {privilegedChip}
      </div>
      <span className="swork-label" role="status" title={label}>
        {label}
      </span>
      <div className="sbase-r">{showCancel && cancelBtn}</div>
    </div>
  );

  // ---- Continuous dictation: what's happening, how long, and a way out ----
  if (state === "dictating") {
    const phase = dictation?.phase ?? "dictating";
    const label =
      editNotice && now < editNoticeUntil
        ? editNotice
        : dictation?.notice
          ? t(DICTATION_NOTICES[dictation.notice])
          : phase === "paused"
            ? t("overlay.dictation.paused")
            : phase === "no_target"
              ? t("overlay.dictation.noTarget")
              : phase === "dictating"
                ? t("overlay.dictation.dictating")
                : t("overlay.dictation.finishing");
    const seconds = dictation
      ? Math.max(0, Math.floor((now - dictation.started_at_ms) / 1000))
      : 0;
    const listening = phase === "dictating";
    const stoppable = phase !== "finishing" && phase !== "ended";
    const failed =
      !!dictation?.notice && ERROR_NOTICES.includes(dictation.notice);
    const look: Look = leaving
      ? "done"
      : failed
        ? "error"
        : phase === "paused" || phase === "no_target"
          ? "paused"
          : listening
            ? "listening"
            : "thinking";

    return (
      <div
        dir={direction}
        className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""} look-${look}`}
        style={stageStyle}
      >
        <div className="spill">
          {glow}
          <div className={`scard compact dictating look-${look}`}>
            <div className="drow">
              {look === "error" ? (
                <PillIcon d={ICONS.warn} className="ready" />
              ) : look === "paused" ? (
                <PillIcon d={ICONS.pause} className="ready" />
              ) : (
                <PillIcon
                  d={PATHS.continuous}
                  className={listening ? "ready" : "arming"}
                />
              )}
              {privilegedChip}
              {listening && (
                <div className="swave ready">
                  {levels.slice(0, 5).map((v, i) => (
                    <i
                      key={i}
                      style={{
                        height: `${Math.max(3, Math.min(18, 3 + Math.pow(v, 0.7) * 15))}px`,
                      }}
                    />
                  ))}
                </div>
              )}
              <span className="dlabel" role="status" title={label}>
                {label}
              </span>
              <span className="stimer">{fmtTime(seconds)}</span>
              {stoppable && (
                <button
                  className="dstop"
                  onClick={() => commands.cancelOperation()}
                >
                  {t("overlay.dictation.stop")}
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ---- Kusema voice editing: why an edit didn't happen ----
  if (state === "notice") {
    return (
      <div
        dir={direction}
        className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""} look-notice`}
        style={stageStyle}
      >
        <div className="spill">
          {glow}
          <div className="scard compact notice look-notice">
            <div className="snotice-row">
              <PillIcon d={ICONS.info} className="ready" />
              <p className="snotice" role="status">
                {editNotice}
              </p>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ---- Live overlay: a pill that sculpts open into a panel ----
  if (state === "streaming") {
    const hasText =
      streamText.committed.length > 0 || streamText.tentative.length > 0;
    const working = phase === "working";
    // While listening, a slow model load opens the panel with a notice where the
    // text would be, so speech that isn't transcribing yet doesn't look ignored.
    const loadingNotice = modelLoadSlow && !working && !hasText;
    // Keep the panel open whenever there's text — even while finalizing — so the
    // transcript stays put under a working spinner instead of collapsing and
    // squishing the text mid-stream. Only fall back to the small working pill
    // when there was no text to preserve.
    const open = hasText || loadingNotice || (loadNoticeShown && !working);
    const collapsed = working && !hasText;

    const look: Look = leaving ? "done" : working ? "thinking" : "listening";
    return (
      <div
        dir={direction}
        className={`ov-stage ${position} ${privileged ? "privileged" : ""} look-${look}`}
        style={stageStyle}
      >
        <div className="spill">
          {glow}
          <div
            key={session}
            className={`scard ${open ? "open" : ""} ${collapsed ? "working" : ""} ${
              isVisible ? "" : "leaving"
            }`}
          >
            <div className="stext">
              <div className="stext-clip">
                <div
                  className={`stext-cap ${overflowing ? "overflowing" : ""}`}
                  ref={capRef}
                  onScroll={handleStreamScroll}
                >
                  <p>
                    {loadingNotice && (
                      <span className="sloading">
                        {t("overlay.loadingModel")}
                      </span>
                    )}
                    <span className="committed">
                      {streamText.committed ? streamText.committed + " " : ""}
                    </span>
                    <span className="tentative">{streamText.tentative}</span>
                    {/* Drop the blinking caret once finalizing — it's no longer
                      capturing, and a static spinner conveys the work. */}
                    {!working && !loadingNotice && <span className="scaret" />}
                  </p>
                </div>
              </div>
            </div>
            {working
              ? workingRow(
                  workKind === "polishing"
                    ? styledLabel(t("overlay.processing"))
                    : transcribingLabel,
                  true,
                )
              : listeningRow(open, true)}
          </div>
        </div>
      </div>
    );
  }

  // ---- Minimal overlay: exactly one row at a time — waveform (recording), or a
  // spinner + label (transcribing / processing / speaking). Never both. The pill
  // animates its width between them; the cancel button is in both rows so it
  // stays put.
  const working =
    state === "transcribing" || state === "processing" || state === "speaking";
  const workLabel =
    state === "processing"
      ? styledLabel(t("overlay.processing"))
      : state === "speaking"
        ? t("overlay.speaking")
        : transcribingLabel;
  const look: Look = leaving
    ? "done"
    : state === "speaking"
      ? "speaking"
      : working
        ? "thinking"
        : "listening";

  return (
    <div
      dir={direction}
      className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""} ${
        privileged ? "privileged" : ""
      } ${assistantTake ? "assistant-take" : ""} look-${look}`}
      style={stageStyle}
    >
      <div className="spill">
        {glow}
        <div
          className={`scard compact ${working && (isVisible || leaving) ? "cworking" : ""} ${
            state === "editing" ? "editing" : ""
          }`}
        >
          {working ? workingRow(workLabel, true) : listeningRow(false, true)}
        </div>
      </div>
    </div>
  );
};

export default RecordingOverlay;
