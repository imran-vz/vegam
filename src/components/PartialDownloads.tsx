import { Loader2, RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import {
	cleanupPartialDownload,
	isApiError,
	listPartialDownloads,
	type ResumeAreaReport,
} from "@/lib/api";
import { formatFileSize, parseError } from "@/lib/utils";

export function PartialDownloads() {
	const [report, setReport] = useState<ResumeAreaReport | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);

	const refresh = useCallback(async () => {
		setBusy(true);
		setError(null);
		try {
			setReport(await listPartialDownloads());
		} catch (err) {
			setError(isApiError(err) ? err.message : parseError(err));
		} finally {
			setBusy(false);
		}
	}, []);

	useEffect(() => {
		refresh();
	}, [refresh]);

	const handleCleanup = async (entryId: string) => {
		setBusy(true);
		setError(null);
		try {
			setReport(await cleanupPartialDownload(entryId));
		} catch (err) {
			setError(isApiError(err) ? err.message : parseError(err));
		} finally {
			setBusy(false);
		}
	};

	return (
		<Card>
			<CardHeader>
				<CardTitle>Partial Downloads</CardTitle>
				<CardDescription>
					Interrupted Transfers keep their progress here so they can resume.
					Vegam never deletes them without you.
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				<div className="flex items-center justify-between">
					<p className="text-sm text-muted-foreground">
						{report
							? `${formatFileSize(report.total_disk_bytes)} in use`
							: "…"}
					</p>
					<Button
						variant="outline"
						size="sm"
						onClick={refresh}
						disabled={busy}
					>
						{busy ? (
							<Loader2 className="size-4 animate-spin" />
						) : (
							<RefreshCw className="size-4" />
						)}
						Refresh
					</Button>
				</div>

				{report && report.entries.length === 0 && (
					<p className="text-sm text-muted-foreground text-center py-4">
						No partial downloads.
					</p>
				)}

				{report?.entries.map((entry) => (
					<div
						key={entry.entry_id}
						className="flex items-center justify-between p-3 bg-muted rounded-lg gap-2"
					>
						<div className="min-w-0">
							<p className="font-medium truncate">
								{entry.file_name ?? "Unknown data"}
							</p>
							<p className="text-xs text-muted-foreground">
								{formatFileSize(entry.disk_bytes)} on disk
								{entry.total_bytes
									? ` · ${Math.round((entry.local_bytes / entry.total_bytes) * 100)}% received`
									: ""}
							</p>
						</div>
						<div className="flex items-center gap-2 shrink-0">
							{!entry.resumable && (
								<Badge variant="destructive">Not resumable</Badge>
							)}
							{entry.stale && entry.resumable && <Badge>Stale</Badge>}
							<Button
								variant="outline"
								size="icon"
								disabled={busy}
								onClick={() => handleCleanup(entry.entry_id)}
							>
								<Trash2 className="size-4" />
							</Button>
						</div>
					</div>
				))}

				{error && (
					<div className="p-3 text-sm text-destructive bg-destructive/10 rounded-lg">
						{error}
					</div>
				)}
			</CardContent>
		</Card>
	);
}
