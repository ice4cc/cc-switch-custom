import React, { useEffect, useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { proxyApi } from "@/lib/api/proxy";
import type { ProxyRequestLogEntry } from "@/types/usage";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  ChevronDown,
  ChevronRight,
  Copy,
  RefreshCw,
  ServerCrash,
  CheckCircle,
} from "lucide-react";

interface RequestLogViewerProps {
  appType?: string;
}

export function RequestLogViewer({ appType }: RequestLogViewerProps) {
  const { t, i18n } = useTranslation();
  const [logs, setLogs] = useState<ProxyRequestLogEntry[]>([]);
  const [expandedIds, setExpandedIds] = useState<Set<number>>(new Set());
  const [copyFeedback, setCopyFeedback] = useState<string | null>(null);

  const fetchLogs = useCallback(async () => {
    try {
      const data = await proxyApi.getRecentProxyLogs(
        appType && appType !== "all" ? appType : undefined,
        100,
      );
      setLogs(data);
    } catch (err) {
      console.error("[RequestLogViewer] Failed to fetch logs:", err);
    }
  }, [appType]);

  useEffect(() => {
    fetchLogs();
    const interval = setInterval(fetchLogs, 5000);
    return () => clearInterval(interval);
  }, [fetchLogs]);

  const toggleExpand = (timestamp: number) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(timestamp)) {
        next.delete(timestamp);
      } else {
        next.add(timestamp);
      }
      return next;
    });
  };

  const copyToClipboard = async (
    text: string,
    label: string,
  ): Promise<void> => {
    try {
      await navigator.clipboard.writeText(text);
      setCopyFeedback(label);
      setTimeout(() => setCopyFeedback(null), 1500);
    } catch {
      // Fallback for older browsers
      const textarea = document.createElement("textarea");
      textarea.value = text;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand("copy");
      document.body.removeChild(textarea);
      setCopyFeedback(label);
      setTimeout(() => setCopyFeedback(null), 1500);
    }
  };

  const formatTime = (ts: number): string => {
    const d = new Date(ts);
    return d.toLocaleTimeString(i18n.resolvedLanguage || "zh", {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  };

  const formatJson = (obj: Record<string, unknown>): string => {
    try {
      return JSON.stringify(obj, null, 2);
    } catch {
      return String(obj);
    }
  };

  const getAppLabel = (type: string): string => {
    const key = `usage.appFilter.${type}`;
    return t(key, type);
  };

  const statusColor = (code: number) => {
    if (code >= 500) return "bg-red-500/20 text-red-600 dark:text-red-400";
    if (code >= 400) return "bg-yellow-500/20 text-yellow-600 dark:text-yellow-400";
    return "bg-green-500/20 text-green-600 dark:text-green-400";
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ServerCrash className="h-4 w-4 text-muted-foreground" />
          <span className="text-sm font-medium text-muted-foreground">
            {t("proxyLog.title", "Real-time Proxy Log")}
          </span>
          <Badge variant="secondary" className="text-xs">
            {logs.length}
          </Badge>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 px-2 text-xs"
          onClick={fetchLogs}
        >
          <RefreshCw className="mr-1 h-3 w-3" />
          {t("common.refresh", "Refresh")}
        </Button>
      </div>

      {logs.length === 0 ? (
        <div className="rounded-lg border border-dashed border-border/50 p-8 text-center">
          <p className="text-sm text-muted-foreground">
            {t("proxyLog.empty", "No proxy requests yet. Send a request through the proxy to see logs here.")}
          </p>
        </div>
      ) : (
        <ScrollArea className="h-[500px] rounded-lg border border-border/50">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[40px]"></TableHead>
                <TableHead className="w-[90px]">{t("usage.time", "Time")}</TableHead>
                <TableHead className="w-[80px]">{t("usage.appFilter.all", "App")}</TableHead>
                <TableHead className="w-[120px]">{t("usage.provider", "Provider")}</TableHead>
                <TableHead className="w-[100px]">{t("usage.requestModel", "Model")}</TableHead>
                <TableHead className="w-[70px]">{t("usage.status", "Status")}</TableHead>
                <TableHead className="w-[80px]">{t("usage.timingInfo", "Latency")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {logs.map((log) => {
                const isExpanded = expandedIds.has(log.timestamp);

                return (
                  <React.Fragment key={log.timestamp}>
                    <TableRow>
                      <TableCell>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 w-6 p-0"
                          onClick={() => toggleExpand(log.timestamp)}
                        >
                          {isExpanded ? (
                            <ChevronDown className="h-4 w-4" />
                          ) : (
                            <ChevronRight className="h-4 w-4" />
                          )}
                        </Button>
                      </TableCell>
                      <TableCell className="text-xs font-mono">
                        {formatTime(log.timestamp)}
                      </TableCell>
                      <TableCell className="text-xs">
                        {getAppLabel(log.appType)}
                      </TableCell>
                      <TableCell className="text-xs">
                        {log.providerName || log.providerId}
                      </TableCell>
                      <TableCell className="text-xs truncate max-w-[120px]" title={log.model}>
                        {log.model}
                      </TableCell>
                      <TableCell>
                        <Badge className={statusColor(log.statusCode)}>
                          {log.statusCode}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-xs">
                        {log.latencyMs}ms
                        {log.success ? (
                          <CheckCircle className="inline h-3 w-3 text-green-500 ml-1" />
                        ) : (
                          <span className="inline h-3 w-3 rounded-full bg-red-500 ml-1" />
                        )}
                      </TableCell>
                    </TableRow>
                    {isExpanded && (
                      <TableRow>
                        <TableCell colSpan={7} className="p-0">
                          <div className="px-6 py-3 space-y-3 bg-muted/20">
                            <div>
                              <div className="flex items-center justify-between mb-1">
                                <span className="text-xs font-medium text-muted-foreground">
                                  {t("proxyLog.request", "Request")}
                                </span>
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  className="h-6 px-2 text-xs"
                                  onClick={() =>
                                    copyToClipboard(
                                      formatJson(log.requestBody),
                                      `req-${log.timestamp}`,
                                    )
                                  }
                                >
                                  <Copy className="mr-1 h-3 w-3" />
                                  {copyFeedback === `req-${log.timestamp}`
                                    ? t("proxyLog.copied", "Copied")
                                    : t("common.copy", "Copy")}
                                </Button>
                              </div>
                              <div className="max-h-[150px] overflow-auto rounded-md bg-background border border-border/50 p-2">
                                <pre className="text-[11px] font-mono whitespace-pre-wrap break-all">
                                  {formatJson(log.requestBody)}
                                </pre>
                              </div>
                            </div>
                            <div>
                              <div className="flex items-center justify-between mb-1">
                                <span className="text-xs font-medium text-muted-foreground">
                                  {t("proxyLog.response", "Response")}
                                </span>
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  className="h-6 px-2 text-xs"
                                  onClick={() =>
                                    copyToClipboard(
                                      formatJson(log.responseBody),
                                      `resp-${log.timestamp}`,
                                    )
                                  }
                                >
                                  <Copy className="mr-1 h-3 w-3" />
                                  {copyFeedback === `resp-${log.timestamp}`
                                    ? t("proxyLog.copied", "Copied")
                                    : t("common.copy", "Copy")}
                                </Button>
                              </div>
                              <div className="max-h-[200px] overflow-auto rounded-md bg-background border border-border/50 p-2">
                                <pre className="text-[11px] font-mono whitespace-pre-wrap break-all">
                                  {formatJson(log.responseBody)}
                                </pre>
                              </div>
                            </div>
                          </div>
                        </TableCell>
                      </TableRow>
                    )}
                  </React.Fragment>
                );
              })}
            </TableBody>
          </Table>
        </ScrollArea>
      )}
    </div>
  );
}
