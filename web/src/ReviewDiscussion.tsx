import { createEffect, createSignal, For, Show } from "solid-js";
import { ArrowLeft, Check, ChevronDown, Clock3, MessageSquare, RefreshCw, Send, ShieldCheck } from "lucide-solid";
import { publicationStatus } from "./sent-requests";
import { StatusBadge, type StatusTone } from "./ui/Arc";
import "./review-discussion.css";

export type ReviewDetail = {
  id: string;
  app_id: string;
  provider: string;
  revision: number;
  state: string;
  request_state?: string;
  configuration?: { name?: string; description?: string };
  scopes?: string[];
  messages?: { id?: string; actor: string; message: string; created_at?: number }[];
  can_decide?: boolean;
  decision?: { decision: string; reason: string; state: string; idempotency_key?: string };
  updated_at?: number;
};
export type ReviewDiscussionProps = {
  review?: ReviewDetail;
  loading?: boolean;
  busy: boolean;
  error?: string;
  currentActor?: string;
  backLabel?: string;
  decision: string;
  reason: string;
  onBack: () => void;
  onRefresh: () => void;
  onReply: (message: string) => Promise<boolean>;
  onDecision: () => void;
  onDecisionChange: (value: string) => void;
  onReasonChange: (value: string) => void;
};
function tone(state: string): StatusTone {
  if (["published", "approved", "accepted"].includes(state)) return "success";
  if (["denied", "failed"].includes(state)) return "danger";
  if (["awaiting_activation", "activating"].includes(state)) return "info";
  return state === "superseded" ? "neutral" : "warning";
}
function reviewState(review: ReviewDetail) {
  return ["published", "denied", "superseded"].includes(review.request_state || "") ? review.request_state! : review.state;
}
function actorName(actor: string) {
  return actor === "provider-review-guidance" ? "Review guidance" : actor;
}
function timestamp(value?: number) {
  if (!value || !Number.isFinite(value)) return "";
  return new Date(value * 1000).toLocaleString(undefined, { year: "numeric", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
}

/** A presentation-only page. The parent retains requests, authorization, and retry keys. */
export default function ReviewDiscussion(props: ReviewDiscussionProps) {
  const [reply, setReply] = createSignal("");
  const [sending, setSending] = createSignal(false);
  const [replyStatus, setReplyStatus] = createSignal("");
  const [replyError, setReplyError] = createSignal("");
  let conversationKey = "";
  let composer: HTMLTextAreaElement | undefined;
  createEffect(() => {
    const key = props.review ? `${props.review.id}:${props.review.provider}` : "";
    // A refresh temporarily clears the loaded detail; keep the in-progress reply.
    if (key && key !== conversationKey) {
      conversationKey = key;
      setReply("");
      setReplyStatus("");
      setReplyError("");
    }
  });
  async function sendReply() {
    const message = reply().trim();
    if (!message || props.busy || sending()) return;
    const key = conversationKey;
    setSending(true);
    setReplyStatus("");
    setReplyError("");
    try {
      const saved = await props.onReply(message);
      if (key !== conversationKey) return;
      if (saved) {
        setReply("");
        setReplyStatus("Reply sent.");
        composer?.focus({ preventScroll: true });
      } else setReplyError("Your reply wasn’t sent. Your draft is still here; try again.");
    } catch (error) {
      if (key === conversationKey) setReplyError(error instanceof Error ? error.message : "Your reply could not be sent. Please try again.");
    } finally {
      setSending(false);
    }
  }
  const decisionDisabled = () => props.busy || (props.decision === "deny" && !props.reason.trim()) || props.review?.decision?.state === "accepted" || (!!props.review?.decision && !props.review.decision.idempotency_key);
  return <section class="discussion-page" aria-label="Application review discussion">
    <div class="discussion-navigation">
      <button class="text-button discussion-back" type="button" onClick={props.onBack}><ArrowLeft size={16} />{props.backLabel || "Back to requests"}</button>
      <button class="button outline discussion-refresh" type="button" disabled={props.busy || props.loading} onClick={props.onRefresh}>
        <RefreshCw size={15} class={props.loading ? "spinning" : ""} />Refresh
      </button>
    </div>
    <Show when={props.error}>
      <div class="discussion-error" role="alert"><strong>Something needs attention</strong><p>{props.error}</p></div>
    </Show>
    <Show when={props.loading && !props.review}>
      <div class="discussion-loading" role="status"><RefreshCw size={20} class="spinning" /><p>Loading discussion…</p></div>
    </Show>
    <Show when={props.review}>{review => <>
      <header class="discussion-heading">
        <div class="discussion-app-mark" aria-hidden="true">{(review().configuration?.name || review().app_id).slice(0, 1).toUpperCase()}</div>
        <div class="discussion-heading-copy">
          <p class="discussion-kicker">Application review</p>
          <h1>{review().configuration?.name || review().app_id}</h1>
          <p class="discussion-subtitle">Revision {review().revision}<span aria-hidden="true">·</span>{review().provider === "honeycomb" ? "Honeycomb publication review" : `${review().provider} permission review`}</p>
        </div>
        <StatusBadge tone={tone(reviewState(review()))}>{publicationStatus(reviewState(review()))}</StatusBadge>
        <Show when={review().can_decide}><a class="discussion-review-jump" href="#discussion-review-context">Review request</a></Show>
      </header>
      <div class="discussion-layout">
        <div class="discussion-main">
          <section class="discussion-conversation" aria-labelledby="discussion-conversation-title" aria-busy={props.loading}>
            <div class="discussion-section-heading">
              <div><MessageSquare size={17} /><h2 id="discussion-conversation-title">Conversation</h2><span class="discussion-message-count">{review().messages?.length || 0}</span></div>
              <button class="text-button discussion-jump" type="button" onClick={() => { composer?.scrollIntoView({ block: "center" }); composer?.focus({ preventScroll: true }); }}>Add a reply</button>
            </div>
            <Show when={review().messages?.length} fallback={<div class="discussion-empty"><span><MessageSquare size={21} /></span><h3>Start the conversation</h3><p>Ask a question or share context to help move this review forward.</p></div>}>
              <ol class="discussion-messages" aria-label="Review messages, oldest first">
                <For each={review().messages}>{message => <li class={`discussion-message ${message.actor === "provider-review-guidance" ? "is-guidance" : ""}`}>
                  <span class="discussion-avatar" aria-hidden="true">
                    <Show when={message.actor === "provider-review-guidance"} fallback={message.actor.replace(/^[cs]:/, "").slice(0, 1).toUpperCase()}><ShieldCheck size={15} /></Show>
                  </span>
                  <article>
                    <div class="discussion-message-meta"><strong>{actorName(message.actor)}</strong><Show when={props.currentActor && message.actor === props.currentActor}><span class="discussion-you">You</span></Show><Show when={timestamp(message.created_at)}><time dateTime={new Date(message.created_at! * 1000).toISOString()}>{timestamp(message.created_at)}</time></Show></div>
                    <p>{message.message}</p>
                  </article>
                </li>}</For>
              </ol>
            </Show>
            <form class="discussion-composer" onSubmit={event => { event.preventDefault(); void sendReply(); }}>
              <label for="discussion-reply">Add a reply</label>
              <textarea ref={composer} id="discussion-reply" name="message" required maxLength={10000} rows={4} placeholder="Ask a question or share an update…" value={reply()} disabled={sending()} onInput={event => { setReply(event.currentTarget.value); setReplyStatus(""); }} onKeyDown={event => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }} aria-describedby="discussion-reply-help" />
              <div class="discussion-composer-footer"><p id="discussion-reply-help">Visible to this application’s owners and reviewers.</p><button class="button primary" disabled={props.busy || sending() || !reply().trim()}><Send size={15} />{sending() ? "Sending…" : "Send reply"}</button></div>
              <Show when={replyError()}><p class="field-error" role="alert">{replyError()}</p></Show>
              <p class="discussion-reply-status" role="status" aria-live="polite">{replyStatus()}</p>
            </form>
          </section>
        </div>
        <aside class="discussion-sidebar" aria-label="Review details and decision">
          <section class="discussion-context" id="discussion-review-context">
            <h2>Request details</h2>
            <dl><div><dt>Application</dt><dd class="discussion-identifier">{review().app_id}</dd></div><div><dt>Reviewer</dt><dd>{review().provider === "honeycomb" ? "Honeycomb" : review().provider}</dd></div><div><dt>Revision</dt><dd>{review().revision}</dd></div><Show when={timestamp(review().updated_at)}><div><dt>Updated</dt><dd>{timestamp(review().updated_at)}</dd></div></Show></dl>
            <Show when={review().configuration?.description}>
              <details class="discussion-disclosure"><summary>Application overview<ChevronDown size={15} /></summary><p>{review().configuration?.description}</p></details>
            </Show>
            <Show when={review().scopes?.length} fallback={<div class="discussion-publication-note"><ShieldCheck size={16} /><p>Publication review. No provider permissions are requested here.</p></div>}>
              <details class="discussion-disclosure discussion-permissions" open={(review().scopes?.length || 0) <= 4}><summary><span>Requested permissions <span class="discussion-permission-count">{review().scopes?.length}</span></span><ChevronDown size={15} /></summary><ul><For each={review().scopes}>{scope => <li><code>{scope}</code></li>}</For></ul></details>
            </Show>
          </section>
          <Show when={review().can_decide} fallback={<section class="discussion-resolution"><Show when={review().decision} fallback={<><Clock3 size={18} /><div><h2>{reviewState(review()) === "superseded" ? "Earlier revision" : "Review status"}</h2><p>{reviewState(review()) === "superseded" ? "A newer revision has replaced this request. The conversation remains available for reference." : "No decision is needed from you on this request. You can still follow the conversation and reply."}</p></div></>}>
            <Show when={review().decision!.decision === "approve"} fallback={<MessageSquare size={18} />}><Check size={18} /></Show><div><h2>{review().decision!.state === "pending" ? "Decision is processing" : review().decision!.decision === "approve" ? "Review approved" : "Review declined"}</h2><Show when={review().decision!.reason}><p class="discussion-decision-reason">{review().decision!.reason}</p></Show><Show when={!review().decision!.reason}><p>The decision has been recorded for this revision.</p></Show></div>
          </Show></section>}>
            <section class="discussion-decision" aria-labelledby="discussion-decision-title">
              <div class="discussion-decision-heading"><span><ShieldCheck size={18} /></span><div><h2 id="discussion-decision-title">Your decision</h2><p>{review().decision ? "Your saved decision" : "This request needs your review"}</p></div></div>
              <form onSubmit={event => { event.preventDefault(); if (!decisionDisabled() && review().can_decide) props.onDecision(); }}>
                <p class="discussion-decision-help">Review the application and requested permissions before deciding.</p>
                <fieldset disabled={!!review().decision || props.busy}><legend>Choose a decision</legend><label class={`discussion-choice ${props.decision === "approve" ? "selected" : ""}`}><input type="radio" name="review-decision" value="approve" checked={props.decision === "approve"} onChange={() => props.onDecisionChange("approve")} /><span><strong>Approve</strong><small>Allow this request to continue.</small></span></label><label class={`discussion-choice ${props.decision === "deny" ? "selected" : ""}`}><input type="radio" name="review-decision" value="deny" checked={props.decision === "deny"} onChange={() => props.onDecisionChange("deny")} /><span><strong>Decline</strong><small>Ask the owner to make changes.</small></span></label></fieldset>
                <label class="discussion-reason-label" for="discussion-reason">{props.decision === "deny" ? "Reason for declining" : "Note to the owner"}<Show when={props.decision !== "deny"}><span>Optional</span></Show></label>
                <textarea id="discussion-reason" required={props.decision === "deny"} maxLength={10000} rows={3} value={props.reason} disabled={!!review().decision || props.busy} placeholder={props.decision === "deny" ? "Explain what needs to change…" : "Add any helpful context…"} onInput={event => props.onReasonChange(event.currentTarget.value)} />
                <button class="button primary discussion-submit-decision" disabled={decisionDisabled()}>{review().decision?.state === "pending" ? "Retry decision" : props.busy ? "Saving…" : props.decision === "deny" ? "Decline request" : "Approve request"}</button>
                <Show when={review().decision?.state === "pending"}><p class="discussion-decision-help">Your decision is saved. Retrying continues the same request.</p></Show>
              </form>
            </section>
          </Show>
        </aside>
      </div>
    </>}</Show>
    <Show when={!props.review && !props.loading && !props.error}><div class="discussion-empty"><MessageSquare size={24} /><h1>Discussion unavailable</h1><p>Return to your requests and open a discussion to continue.</p></div></Show>
  </section>;
}
