/** The Ask box's rules, kept plain so they can be checked on their own. */

export type AnswerStatus = "idle" | "thinking" | "done" | "error";

export interface AnswerState {
  /** The question being answered; events for other questions are ignored. */
  id: number | null;
  question: string;
  text: string;
  status: AnswerStatus;
}

export type AssistantEvent = {
  kind: "stream" | "reply" | "error" | "ended";
  id: number;
  text: string;
};

/** The answer after an event about some question. */
export const answerFor = (
  current: AnswerState,
  event: AssistantEvent,
): AnswerState => {
  if (current.id === null || event.id !== current.id) return current;
  switch (event.kind) {
    case "stream":
      return current.status === "thinking"
        ? { ...current, text: current.text + event.text }
        : current;
    case "reply":
      return { ...current, text: event.text, status: "done" };
    case "error":
      return { ...current, text: event.text, status: "error" };
    case "ended":
      return { ...current, text: "", status: "idle" };
  }
};

/** Events that came before the question's id did (a quick error), replayed. */
export const replay = (
  started: AnswerState,
  early: AssistantEvent[],
): AnswerState => early.reduce(answerFor, started);

/** What a key does in the box: Escape closes, Enter asks, Shift+Enter is a new line. */
export const boxKey = (
  key: string,
  shift: boolean,
): "close" | "submit" | null => {
  if (key === "Escape") return "close";
  if (key === "Enter" && !shift) return "submit";
  return null;
};
