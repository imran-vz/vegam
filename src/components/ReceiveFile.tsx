import { save } from "@tauri-apps/plugin-dialog";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { Download, Loader2, Pause, Play, X } from "lucide-react";
import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import {
	cancelReceiveTransfer,
	createReceiveTransfer,
	inspectTicket,
	isApiError,
	listenToReceiveTransferProgress,
	listenToReceiveTransferUpdates,
	pauseReceiveTransfer,
	type ReceiveProgress,
	resumeReceiveTransfer,
	type ReceiveTransferInfo,
} from "@/lib/api";
import { formatFileSize, formatTransferSpeed, parseError } from "@/lib/utils";

const STATUS_LABELS: Record<ReceiveTransferInfo["status"], string> = {
	connecting: "Connecting",
	downloading: "Downloading",
	paused: "Paused",
	stalledRetrying: "Waiting for sender",
	verifying: "Verifying",
	exporting: "Saving",
	complete: "Completed",
	failed: "Failed",
	cancelled: "Cancelled",
	noLongerResumable: "No longer available",
};

interface ReceiveFileProps {
	initialTransfers: ReceiveTransferInfo[];
}

export function ReceiveFile({ initialTransfers }: ReceiveFileProps) {
	const [ticket, setTicket] = useState("");
	const [transfer, setTransfer] = useState<ReceiveTransferInfo | null>(
		initialTransfers[0] ?? null,
	);
	const [progress, setProgress] = useState<ReceiveProgress | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);

	useEffect(() => {
		const unlistenUpdate = listenToReceiveTransferUpdates((updated) => {
			setTransfer((current) => {
				if (current === null || current.id === updated.id) {
					return updated.status === "cancelled" ? null : updated;
				}
				return current;
			});
		});
		const unlistenProgress = listenToReceiveTransferProgress((p) => {
			setProgress((current) => {
				return current === null || current.id === p.id ? p : current;
			});
		});
		return () => {
			unlistenUpdate.then((fn) => fn());
			unlistenProgress.then((fn) => fn());
		};
	}, []);

	const handlePaste = async () => {
		try {
			const text = await readText();
			if (text?.trim()) {
				setTicket(text.trim());
			}
		} catch (err) {
			setError(parseError(err));
		}
	};

	const handleReceive = async () => {
		if (!ticket.trim()) return;
		setError(null);
		setBusy(true);
		try {
			const preview = await inspectTicket(ticket.trim());
			const selectedPath = await save({
				defaultPath: `Downloads/${preview.file_name}`,
			});
			if (!selectedPath) return;
			const info = await createReceiveTransfer(ticket.trim(), selectedPath);
			setTransfer(info);
			setProgress(null);
			setTicket("");
		} catch (err) {
			setError(isApiError(err) ? err.message : parseError(err));
		} finally {
			setBusy(false);
		}
	};

	const withTransfer = async (
		action: (id: string) => Promise<unknown>,
	): Promise<void> => {
		if (!transfer) return;
		setError(null);
		try {
			await action(transfer.id);
		} catch (err) {
			setError(isApiError(err) ? err.message : parseError(err));
		}
	};

	const handleCancel = async () => {
		await withTransfer(cancelReceiveTransfer);
		setTransfer(null);
		setProgress(null);
	};

	const localBytes =
		progress && transfer && progress.id === transfer.id
			? progress.local_bytes
			: (transfer?.local_bytes ?? 0);
	const totalBytes = transfer?.size ?? 0;
	const percent =
		totalBytes > 0 ? Math.round((localBytes / totalBytes) * 100) : 0;
	const isActive =
		transfer !== null &&
		["connecting", "downloading", "stalledRetrying", "verifying", "exporting"].includes(
			transfer.status,
		);

	return (
		<Card>
			<CardHeader>
				<CardTitle>Receive File</CardTitle>
				<CardDescription>
					Paste a Transfer Ticket to receive a file
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				{transfer === null ? (
					<>
						<div className="space-y-2">
							<textarea
								value={ticket}
								onChange={(e) => setTicket(e.target.value)}
								placeholder="Paste transfer ticket here..."
								className="w-full h-24 p-3 text-sm font-mono border rounded-lg resize-none"
							/>
							<Button
								variant="outline"
								size="sm"
								onClick={handlePaste}
								className="w-full"
							>
								Paste from Clipboard
							</Button>
						</div>

						<Button
							onClick={handleReceive}
							disabled={busy || !ticket.trim()}
							className="w-full"
						>
							{busy ? (
								<Loader2 className="size-4 animate-spin" />
							) : (
								<Download className="size-4" />
							)}
							Receive
						</Button>
					</>
				) : (
					<div className="space-y-4">
						<div className="flex items-center justify-between p-3 bg-muted rounded-lg">
							<div>
								<p className="font-medium">{transfer.file_name}</p>
								<p className="text-sm text-muted-foreground">
									{formatFileSize(transfer.size)}
								</p>
							</div>
							<Badge>{STATUS_LABELS[transfer.status]}</Badge>
						</div>

						{transfer.status === "complete" ? (
							<div className="p-3 text-sm text-green-700 bg-green-100 rounded-lg">
								<p className="font-medium">Transfer Completed</p>
							</div>
						) : (
							<div className="space-y-2">
								<div className="flex justify-between text-sm">
									<span>{STATUS_LABELS[transfer.status]}…</span>
									<span>{percent}%</span>
								</div>
								<Progress value={percent} />
								<div className="flex justify-between text-xs text-muted-foreground">
									<span>
										{transfer.connection_kind === "direct"
											? "Direct"
											: transfer.connection_kind === "relayed"
												? "Relayed"
												: ""}
									</span>
									{progress && progress.speed_bps > 0 && isActive && (
										<span>{formatTransferSpeed(progress.speed_bps)}</span>
									)}
								</div>
							</div>
						)}

						<div className="flex gap-2">
							{isActive && (
								<Button
									variant="outline"
									className="flex-1"
									onClick={() => withTransfer(pauseReceiveTransfer)}
								>
									<Pause className="size-4" />
									Pause
								</Button>
							)}
							{["paused", "failed", "noLongerResumable"].includes(
								transfer.status,
							) && (
								<Button
									variant="outline"
									className="flex-1"
									onClick={() => withTransfer(resumeReceiveTransfer)}
								>
									<Play className="size-4" />
									Resume
								</Button>
							)}
							{transfer.status === "complete" ? (
								<Button
									variant="outline"
									className="flex-1"
									onClick={() => {
										setTransfer(null);
										setProgress(null);
									}}
								>
									Receive Another File
								</Button>
							) : (
								<Button
									variant="outline"
									className="flex-1"
									onClick={handleCancel}
								>
									<X className="size-4" />
									Cancel
								</Button>
							)}
						</div>
					</div>
				)}

				{(error || transfer?.error) && (
					<div className="p-3 text-sm text-destructive bg-destructive/10 rounded-lg">
						{error ?? transfer?.error}
					</div>
				)}
			</CardContent>
		</Card>
	);
}
