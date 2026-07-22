import { useContext, useEffect, useState } from "react";
import { OperatorAssessment } from "../../api-types";
import { APIContext } from "../../context";
import { customSetInterval } from "../../helper/customSetInterval";

interface OperatorTabProps {
  torrentId: number;
}

const EMPTY_STATE_TEXT =
  "No assessment yet — the operator may be disabled, idle (no active torrents), or hasn't evaluated this torrent.";

const concernBadgeClass = (concern: string): string => {
  switch (concern) {
    case "problem":
      return "bg-red-100 text-red-800 dark:bg-red-900/40 dark:text-red-300";
    case "watch":
      return "bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-300";
    case "none":
    default:
      return "bg-green-100 text-green-800 dark:bg-green-900/40 dark:text-green-300";
  }
};

export const OperatorTab: React.FC<OperatorTabProps> = ({ torrentId }) => {
  const API = useContext(APIContext);
  const [assessment, setAssessment] = useState<OperatorAssessment | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    setLoaded(false);
    setAssessment(null);

    return customSetInterval(() => {
      return API.getOperatorAssessments().then(
        (resp) => {
          const found =
            resp.assessments.find((a) => a.torrent_idx === torrentId) ?? null;
          setAssessment(found);
          setLoaded(true);
          return 5000;
        },
        (err) => {
          console.error(err);
          setLoaded(true);
          return 5000;
        },
      );
    }, 0);
  }, [torrentId]);

  if (!loaded && !assessment) {
    return <div className="p-4 text-tertiary">Loading...</div>;
  }

  if (!assessment) {
    return <div className="p-4 text-tertiary">{EMPTY_STATE_TEXT}</div>;
  }

  return (
    <div className="p-4 space-y-3">
      <div>
        <span
          className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium capitalize ${concernBadgeClass(
            assessment.concern,
          )}`}
        >
          {assessment.concern}
        </span>
      </div>
      <p className="text-sm text-secondary whitespace-pre-wrap">
        {assessment.summary}
      </p>
    </div>
  );
};
