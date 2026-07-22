import { useContext, useEffect, useState } from "react";
import { APIContext } from "../../context";
import {
  OperatorConfirmation,
  OperatorDecision,
  OperatorTier,
} from "../../api-types";
import { customSetInterval } from "../../helper/customSetInterval";
import { Modal } from "./Modal";
import { ModalBody } from "./ModalBody";
import { ModalFooter } from "./ModalFooter";
import { Button } from "../buttons/Button";
import { Spinner } from "../Spinner";

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

export const OperatorModal: React.FC<Props> = ({ show, onClose }) => {
  const API = useContext(APIContext);

  const [decisions, setDecisions] = useState<OperatorDecision[] | null>(null);
  const [confirmations, setConfirmations] = useState<
    OperatorConfirmation[] | null
  >(null);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);

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
      </ModalBody>
      <ModalFooter>
        <Button variant="primary" onClick={onClose}>
          Close
        </Button>
      </ModalFooter>
    </Modal>
  );
};
