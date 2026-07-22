import { createContext } from "react";
import {
  LimitsConfig,
  OperatorActionResponse,
  OperatorConfig,
  OperatorConfigResponse,
  OperatorAssessmentsResponse,
  OperatorConfirmationsResponse,
  OperatorDecisionsResponse,
  RqbitAPI,
  SessionStats,
} from "./api-types";

export const APIContext = createContext<RqbitAPI>({
  listTorrents: () => {
    throw new Error("Function not implemented.");
  },
  getTorrentDetails: () => {
    throw new Error("Function not implemented.");
  },
  getTorrentStats: () => {
    throw new Error("Function not implemented.");
  },
  getPeerStats: () => {
    throw new Error("Function not implemented.");
  },
  uploadTorrent: () => {
    throw new Error("Function not implemented.");
  },
  updateOnlyFiles: () => {
    throw new Error("Function not implemented.");
  },
  pause: () => {
    throw new Error("Function not implemented.");
  },
  start: () => {
    throw new Error("Function not implemented.");
  },
  forget: () => {
    throw new Error("Function not implemented.");
  },
  delete: () => {
    throw new Error("Function not implemented.");
  },
  getTorrentStreamUrl: () => {
    throw new Error("Function not implemented.");
  },
  getStreamLogsUrl: function (): string | null {
    throw new Error("Function not implemented.");
  },
  getPlaylistUrl: function (index: number): string | null {
    throw new Error("Function not implemented.");
  },
  stats: function (): Promise<SessionStats> {
    throw new Error("Function not implemented.");
  },
  getTorrentHaves: function (index: number): Promise<Uint8Array> {
    throw new Error("Function not implemented.");
  },
  getLimits: function (): Promise<LimitsConfig> {
    throw new Error("Function not implemented.");
  },
  setLimits: function (limits: LimitsConfig): Promise<void> {
    throw new Error("Function not implemented.");
  },
  getOperatorDecisions: function (): Promise<OperatorDecisionsResponse> {
    throw new Error("Function not implemented.");
  },
  getOperatorConfirmations:
    function (): Promise<OperatorConfirmationsResponse> {
      throw new Error("Function not implemented.");
    },
  operatorApprove: function (id: number): Promise<OperatorActionResponse> {
    throw new Error("Function not implemented.");
  },
  operatorReject: function (id: number): Promise<OperatorActionResponse> {
    throw new Error("Function not implemented.");
  },
  getOperatorConfig: function (): Promise<OperatorConfigResponse> {
    throw new Error("Function not implemented.");
  },
  setOperatorConfig: function (
    config: OperatorConfig,
  ): Promise<{ status: string; note: string }> {
    throw new Error("Function not implemented.");
  },
  getOperatorAssessments: function (): Promise<OperatorAssessmentsResponse> {
    throw new Error("Function not implemented.");
  },
});
