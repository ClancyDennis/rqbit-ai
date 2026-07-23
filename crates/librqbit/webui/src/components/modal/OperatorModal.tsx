import { useContext, useEffect, useState } from "react";
import { APIContext } from "../../context";
import {
  OperatorAssessment,
  OperatorConfig,
  OperatorConfirmation,
  OperatorDecision,
  OperatorEvaluation,
  OperatorTier,
} from "../../api-types";
import { customSetInterval } from "../../helper/customSetInterval";
import { Modal } from "./Modal";
import { ModalBody } from "./ModalBody";
import { ModalFooter } from "./ModalFooter";
import { Button } from "../buttons/Button";
import { Spinner } from "../Spinner";
import { FormInput } from "../forms/FormInput";
import { FormCheckbox } from "../forms/FormCheckbox";

interface Props {
  show: boolean;
  onClose: () => void;
}

const TIER_BADGE: Record<OperatorTier, string> = {
  auto: "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200",
  notify: "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200",
  confirm: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200",
};

const TierBadge: React.FC<{ tier: OperatorTier }> = ({ tier }) => (
  <span
    className={`inline-block rounded px-1.5 py-0.5 text-xs font-semibold uppercase tracking-wide ${
      TIER_BADGE[tier] ??
      "bg-slate-100 text-slate-800 dark:bg-slate-700 dark:text-slate-200"
    }`}
  >
    {tier}
  </span>
);

const RunningBadge: React.FC<{ running: boolean }> = ({ running }) => (
  <span
    className={`inline-block rounded px-1.5 py-0.5 text-xs font-semibold uppercase tracking-wide ${
      running
        ? "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
        : "bg-slate-100 text-slate-800 dark:bg-slate-700 dark:text-slate-200"
    }`}
  >
    {running ? "running" : "not running"}
  </span>
);

const torrentLabel = (idx: number | null): string =>
  idx === null ? "session" : `#${idx}`;

const ConfirmationRow: React.FC<{
  confirmation: OperatorConfirmation;
  onApprove: (id: number) => void;
  onReject: (id: number) => void;
  busy: boolean;
}> = ({ confirmation, onApprove, onReject, busy }) => (
  <div className="flex flex-col gap-2 rounded border border-divider bg-surface p-3 dark:bg-slate-800 sm:flex-row sm:items-center sm:justify-between">
    <div className="min-w-0">
      <div className="flex items-center gap-2">
        <span className="font-semibold">{confirmation.kind}</span>
        <span className="text-xs text-secondary">
          torrent {torrentLabel(confirmation.torrent_idx)}
        </span>
      </div>
      <p className="mt-0.5 text-sm text-secondary">{confirmation.rationale}</p>
    </div>
    <div className="flex flex-shrink-0 gap-2">
      <Button
        variant="primary"
        size="sm"
        disabled={busy}
        onClick={() => onApprove(confirmation.id)}
      >
        Approve
      </Button>
      <Button
        variant="danger"
        size="sm"
        disabled={busy}
        onClick={() => onReject(confirmation.id)}
      >
        Reject
      </Button>
    </div>
  </div>
);

const DecisionRow: React.FC<{ decision: OperatorDecision }> = ({
  decision,
}) => (
  <div className="rounded border border-divider bg-surface p-2 dark:bg-slate-800">
    <div className="flex flex-wrap items-center gap-2">
      <TierBadge tier={decision.tier} />
      <span className="font-semibold">{decision.kind}</span>
      <span className="text-xs text-secondary">
        torrent {torrentLabel(decision.torrent_idx)}
      </span>
      {decision.confidence !== null && (
        <span className="text-xs text-secondary">
          confidence {Math.round(decision.confidence * 100)}%
        </span>
      )}
      <span className="ml-auto text-xs text-secondary">{decision.outcome}</span>
    </div>
    <p className="mt-1 text-sm text-secondary">{decision.rationale}</p>
  </div>
);

const AssessmentRow: React.FC<{ assessment: OperatorAssessment }> = ({
  assessment,
}) => (
  <div className="rounded border border-divider bg-surface p-2 dark:bg-slate-800">
    <div className="flex flex-wrap items-center gap-2">
      <span className="text-xs text-secondary">
        torrent {torrentLabel(assessment.torrent_idx)}
      </span>
      {assessment.concern && (
        <span className="inline-block rounded bg-amber-100 px-1.5 py-0.5 text-xs font-semibold text-amber-800 dark:bg-amber-900 dark:text-amber-200">
          {assessment.concern}
        </span>
      )}
    </div>
    <p className="mt-1 text-sm text-secondary">{assessment.summary}</p>
  </div>
);

const formatPerTorrent = (n: number): string =>
  Number.isFinite(n) ? n.toFixed(1) : "0.0";

export const OperatorModal: React.FC<Props> = ({ show, onClose }) => {
  const API = useContext(APIContext);

  const [decisions, setDecisions] = useState<OperatorDecision[] | null>(null);
  const [confirmations, setConfirmations] = useState<
    OperatorConfirmation[] | null
  >(null);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);

  const [config, setConfig] = useState<OperatorConfig | null>(null);
  const [running, setRunning] = useState<boolean>(false);
  const [saving, setSaving] = useState<boolean>(false);
  const [saveNote, setSaveNote] = useState<string | null>(null);

  // "Evaluate now" testing/diagnostics state.
  const [evaluating, setEvaluating] = useState<boolean>(false);
  const [evaluation, setEvaluation] = useState<OperatorEvaluation | null>(null);
  const [evalError, setEvalError] = useState<string | null>(null);

  const [snapshot, setSnapshot] = useState<string | null>(null);
  const [showSnapshot, setShowSnapshot] = useState<boolean>(false);
  const [snapshotLoading, setSnapshotLoading] = useState<boolean>(false);
  const [snapshotError, setSnapshotError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const [d, c] = await Promise.all([
        API.getOperatorDecisions(),
        API.getOperatorConfirmations(),
      ]);
      setDecisions(d.decisions);
      setConfirmations(c.confirmations);
      setError(null);
    } catch (e: any) {
      setError(e?.text ? String(e.text) : "Failed to load operator data");
      console.error(e);
    }
  };

  // Poll both lists every ~5s while the modal is open.
  useEffect(() => {
    if (!show) {
      return;
    }
    return customSetInterval(async () => {
      await refresh();
      return 5000;
    }, 0);
  }, [show]);

  // Load the operator config once when the modal opens.
  useEffect(() => {
    if (!show) {
      return;
    }
    setSaveNote(null);
    API.getOperatorConfig()
      .then((r) => {
        setConfig(r.config);
        setRunning(r.running);
      })
      .catch((e: any) => {
        setError(e?.text ? String(e.text) : "Failed to load operator config");
        console.error(e);
      });
  }, [show]);

  const updateConfig = <K extends keyof OperatorConfig>(
    key: K,
    value: OperatorConfig[K],
  ) => {
    setConfig((prev) => (prev === null ? prev : { ...prev, [key]: value }));
  };

  const handleSave = async () => {
    if (config === null) {
      return;
    }
    setSaving(true);
    setSaveNote(null);
    try {
      const r = await API.setOperatorConfig(config);
      setSaveNote(r.note);
      setError(null);
    } catch (e: any) {
      setError(e?.text ? String(e.text) : "Failed to save operator config");
      console.error(e);
    } finally {
      setSaving(false);
    }
  };

  const handleApprove = async (id: number) => {
    setBusyId(id);
    try {
      await API.operatorApprove(id);
      await refresh();
    } catch (e: any) {
      setError(e?.text ? String(e.text) : "Failed to approve");
      console.error(e);
    } finally {
      setBusyId(null);
    }
  };

  const handleReject = async (id: number) => {
    setBusyId(id);
    try {
      await API.operatorReject(id);
      await refresh();
    } catch (e: any) {
      setError(e?.text ? String(e.text) : "Failed to reject");
      console.error(e);
    } finally {
      setBusyId(null);
    }
  };

  const handleEvaluate = async () => {
    setEvaluating(true);
    setEvalError(null);
    try {
      const r = await API.operatorEvaluate();
      setEvaluation(r);
    } catch (e: any) {
      setEvalError(e?.text ? String(e.text) : "Evaluation failed");
      console.error(e);
    } finally {
      setEvaluating(false);
    }
  };

  const handleToggleSnapshot = async () => {
    if (showSnapshot) {
      setShowSnapshot(false);
      return;
    }
    setShowSnapshot(true);
    setSnapshotLoading(true);
    setSnapshotError(null);
    try {
      const r = await API.getOperatorSnapshot();
      setSnapshot(JSON.stringify(r, null, 2));
    } catch (e: any) {
      setSnapshotError(e?.text ? String(e.text) : "Failed to load snapshot");
      console.error(e);
    } finally {
      setSnapshotLoading(false);
    }
  };

  return (
    <Modal isOpen={show} onClose={onClose} title="AI operator">
      <ModalBody>
        {error && (
          <div className="mb-3 rounded border border-red-300 bg-red-50 p-2 text-sm text-red-800 dark:border-red-800 dark:bg-red-950 dark:text-red-200">
            {error}
          </div>
        )}

        <section className="mb-5">
          <h3 className="mb-2 text-lg font-semibold">Pending confirmations</h3>
          {confirmations === null ? (
            <Spinner label="Loading" />
          ) : confirmations.length === 0 ? (
            <p className="text-sm text-secondary">
              Nothing needs your approval right now.
            </p>
          ) : (
            <div className="flex flex-col gap-2">
              {confirmations.map((c) => (
                <ConfirmationRow
                  key={c.id}
                  confirmation={c}
                  onApprove={handleApprove}
                  onReject={handleReject}
                  busy={busyId === c.id}
                />
              ))}
            </div>
          )}
        </section>

        <section>
          <h3 className="mb-2 text-lg font-semibold">Recent decisions</h3>
          {decisions === null ? (
            <Spinner label="Loading" />
          ) : decisions.length === 0 ? (
            <p className="text-sm text-secondary">No decisions yet.</p>
          ) : (
            <div className="flex max-h-80 flex-col gap-2 overflow-y-auto pr-1">
              {decisions.map((d) => (
                <DecisionRow key={d.seq} decision={d} />
              ))}
            </div>
          )}
        </section>

        <section className="mt-5">
          <div className="mb-2 flex items-center gap-2">
            <h3 className="text-lg font-semibold">Settings</h3>
            <RunningBadge running={running} />
          </div>
          {config === null ? (
            <Spinner label="Loading" />
          ) : (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                handleSave();
              }}
              className="flex flex-col gap-3"
            >
              <FormCheckbox
                name="operator-enabled"
                label="Enabled"
                help="Turn the AI operator on or off."
                checked={config.enabled}
                onChange={(e) => updateConfig("enabled", e.target.checked)}
              />
              <FormCheckbox
                name="operator-live"
                label="Live (apply actions)"
                help="When on, the operator takes real actions on your torrents instead of only simulating them. Leave off to keep it in a safe dry-run mode."
                checked={!config.dry_run}
                onChange={(e) => updateConfig("dry_run", !e.target.checked)}
              />
              {!config.dry_run && (
                <div className="rounded border border-amber-300 bg-amber-50 p-2 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200">
                  Live mode lets the operator take actions (pause, recheck,
                  delete, adjust limits) on your behalf.
                </div>
              )}
              <FormInput
                name="operator-base-url"
                label="Base URL"
                inputType="text"
                placeholder="http://localhost:4000"
                value={config.base_url}
                onChange={(e) => updateConfig("base_url", e.target.value)}
              />
              <FormInput
                name="operator-model"
                label="Model"
                inputType="text"
                placeholder="gpt-5.6-luna-global"
                value={config.model}
                onChange={(e) => updateConfig("model", e.target.value)}
              />
              <FormInput
                name="operator-poll-interval"
                label="Poll interval (seconds)"
                inputType="number"
                value={String(config.poll_interval_secs)}
                onChange={(e) =>
                  updateConfig(
                    "poll_interval_secs",
                    Math.max(1, Number(e.target.value) || 1),
                  )
                }
              />
              <FormInput
                name="operator-asn-db-path"
                label="ASN database path (optional)"
                inputType="text"
                placeholder="/path/to/asn.mmdb"
                value={config.asn_db_path ?? ""}
                onChange={(e) =>
                  updateConfig(
                    "asn_db_path",
                    e.target.value === "" ? null : e.target.value,
                  )
                }
              />
              <p className="text-sm text-secondary">
                API key is read from the RQBIT_OPERATOR_API_KEY environment
                variable.
              </p>
              <div className="flex items-center gap-3">
                <Button
                  variant="primary"
                  disabled={saving}
                  onClick={handleSave}
                >
                  Save
                </Button>
                {saveNote && (
                  <span className="text-sm text-secondary">{saveNote}</span>
                )}
              </div>
            </form>
          )}

          <div className="mt-5 rounded border border-divider bg-surface p-3 dark:bg-slate-800">
            <div className="mb-1 flex items-center gap-2">
              <h4 className="text-base font-semibold">Evaluate (test)</h4>
              <span className="inline-block rounded bg-slate-100 px-1.5 py-0.5 text-xs font-semibold uppercase tracking-wide text-slate-700 dark:bg-slate-700 dark:text-slate-200">
                diagnostics
              </span>
            </div>
            <p className="mb-3 text-sm text-secondary">
              Run one decision against the configured model on the current state
              to A/B different models and estimate token cost. Nothing is
              executed.
            </p>

            <div className="flex flex-wrap items-center gap-3">
              <Button
                variant="primary"
                size="sm"
                disabled={evaluating}
                onClick={handleEvaluate}
              >
                {evaluating ? "Evaluating…" : "Evaluate now"}
              </Button>
              {evaluating && <Spinner />}
              <Button
                variant="secondary"
                size="sm"
                disabled={snapshotLoading}
                onClick={handleToggleSnapshot}
              >
                {showSnapshot ? "Hide snapshot" : "View snapshot"}
              </Button>
            </div>

            {evalError && (
              <div className="mt-3 rounded border border-red-300 bg-red-50 p-2 text-sm text-red-800 dark:border-red-800 dark:bg-red-950 dark:text-red-200">
                {evalError}
              </div>
            )}

            {evaluation && !evaluating && (
              <div className="mt-3 flex flex-col gap-3">
                <div className="rounded border border-divider bg-white p-3 dark:bg-slate-900">
                  <div className="mb-1 text-sm font-semibold">Token usage</div>
                  {evaluation.usage === null ? (
                    <p className="text-sm text-secondary">
                      usage not reported by endpoint
                    </p>
                  ) : (
                    <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
                      <span className="text-2xl font-bold">
                        {evaluation.usage.total_tokens}
                        <span className="ml-1 text-sm font-normal text-secondary">
                          total tokens
                        </span>
                      </span>
                      <span className="text-sm text-secondary">
                        {formatPerTorrent(evaluation.tokens_per_torrent)} tokens
                        / torrent ({evaluation.torrents} torrents)
                      </span>
                      <span className="text-sm text-secondary">
                        prompt {evaluation.usage.prompt_tokens} · completion{" "}
                        {evaluation.usage.completion_tokens}
                      </span>
                    </div>
                  )}
                  {evaluation.usage === null && (
                    <p className="mt-1 text-sm text-secondary">
                      {formatPerTorrent(evaluation.tokens_per_torrent)} tokens /
                      torrent ({evaluation.torrents} torrents)
                    </p>
                  )}
                </div>

                <div>
                  <div className="mb-1 text-sm font-semibold">
                    Assessments ({evaluation.assessments.length}) · decisions (
                    {evaluation.decisions.length})
                  </div>
                  {evaluation.assessments.length === 0 ? (
                    <p className="text-sm text-secondary">
                      No assessments returned.
                    </p>
                  ) : (
                    <div className="flex max-h-60 flex-col gap-2 overflow-y-auto pr-1">
                      {evaluation.assessments.map((a, i) => (
                        <AssessmentRow key={i} assessment={a} />
                      ))}
                    </div>
                  )}
                </div>

                <div>
                  <div className="mb-1 text-sm font-semibold">Raw response</div>
                  <pre className="max-h-64 overflow-auto rounded border border-divider bg-slate-50 p-2 font-mono text-xs text-slate-800 dark:bg-slate-950 dark:text-slate-200">
                    {evaluation.raw_response}
                  </pre>
                </div>
              </div>
            )}

            {showSnapshot && (
              <div className="mt-3">
                <div className="mb-1 text-sm font-semibold">
                  State snapshot (fed to model)
                </div>
                {snapshotLoading ? (
                  <Spinner label="Loading" />
                ) : snapshotError ? (
                  <div className="rounded border border-red-300 bg-red-50 p-2 text-sm text-red-800 dark:border-red-800 dark:bg-red-950 dark:text-red-200">
                    {snapshotError}
                  </div>
                ) : (
                  <pre className="max-h-64 overflow-auto rounded border border-divider bg-slate-50 p-2 font-mono text-xs text-slate-800 dark:bg-slate-950 dark:text-slate-200">
                    {snapshot}
                  </pre>
                )}
              </div>
            )}
          </div>
        </section>
      </ModalBody>
      <ModalFooter>
        <Button variant="primary" onClick={onClose}>
          Close
        </Button>
      </ModalFooter>
    </Modal>
  );
};
