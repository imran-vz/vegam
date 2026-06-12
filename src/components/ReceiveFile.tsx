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

const ACTIVE_STATUSES: ReceiveTransferInfo["status"][] = [
	"connecting",
	"downloading",
	"stalledRetrying",
	"verifying",
	"exporting",
];

interface ReceiveFileProps {
	initialTransfers: ReceiveTransferInfo[];
}

export function ReceiveFile({ initialTransfers }: ReceiveFileProps) {
	const [ticket, setTicket] = useState("");
	const [transfers, setTransfers] = useState<ReceiveTransferInfo[]>(
		initialTransfers,
	);
	const [progressById, setProgressById] = useState<
		Record<string, ReceiveProgress>
	>({});
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);

	useEffect(() => {
		const unlistenUpdate = listenToReceiveTransferUpdates((updated) => {
			setTransfers((current) => {
				const without = current.filter((t) => t.id !== updated.id);
				if (updated.status === "cancelled") {
					return without;
				}
				return [...without, updated].sort((a, b) =>
					a.id.localeCompare(b.id),
				);
			});
		});
		const unlistenProgress = listenToReceiveTransferProgress((p) => {
			setProgressById((current) => ({ ...current, [p.id]: p }));
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
			setTransfers((current) => [
				...current.filter((t) => t.id !== info.id),
				info,
			]);
			setTicket("");
		} catch (err) {
			setError(isApiError(err) ? err.message : parseError(err));
		} finally {
			setBusy(false);
		}
	};

	const removeLocal = (id: string) => {
		setTransfers((current) => current.filter((t) => t.id !== id));
	};

	return (
		<Card>
			<CardHeader>
				<CardTitle>Receive File</CardTitle>
				<CardDescription>
					Paste a Transfer Ticket to receive a file
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				<div className="space-y-2">
					<textarea
						value={ticket}
						onChange={(e) => setTicket(e.target.value)}
						placeholder="Paste transfer ticket here..."
						className="w-full h-24 p-3 text-sm font-mono border rounded-lg resize-none"
					/>
					<div className="grid grid-cols-2 gap-2">
						<Button
							variant="outline"
							size="sm"
							onClick={handlePaste}
							className="w-full"
						>
							Paste from Clipboard
						</Button>
						<Button
							onClick={handleReceive}
							disabled={busy || !ticket.trim()}
							size="sm"
							className="w-full"
						>
							{busy ? (
								<Loader2 className="size-4 animate-spin" />
							) : (
								<Download className="size-4" />
							)}
							Receive
						</Button>
					</div>
				</div>

				{transfers.map((transfer) => (
					<ReceiveTransferCard
						key={transfer.id}
						transfer={transfer}
						progress={progressById[transfer.id]}
						onRemoved={() => removeLocal(transfer.id)}
						onError={setError}
					/>
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

function ReceiveTransferCard({
	transfer,
	progress,
	onRemoved,
	onError,
}: {
	transfer: ReceiveTransferInfo;
	progress: ReceiveProgress | undefined;
	onRemoved: () => void;
	onError: (message: string | null) => void;
}) {
	const act = async (action: () => Promise<unknown>) => {
		onError(null);
		try {
			await action();
		} catch (err) {
			onError(isApiError(err) ? err.message : parseError(err));
		}
	};

	const handleCancel = async () => {
		await act(() => cancelReceiveTransfer(transfer.id));
		onRemoved();
	};

	const isActive = ACTIVE_STATUSES.includes(transfer.status);
	const localBytes = progress?.local_bytes ?? transfer.local_bytes;
	const percent =
		transfer.size > 0 ? Math.round((localBytes / transfer.size) * 100) : 0;
	const kind = progress?.connection_kind ?? transfer.connection_kind;

	return (
		<div className="space-y-3 p-3 border rounded-lg">
			<div className="flex items-center justify-between">
				<div className="min-w-0">
					<p className="font-medium truncate">{transfer.file_name}</p>
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
							{kind === "direct"
								? "Direct"
								: kind === "relayed"
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
						size="sm"
						className="flex-1"
						onClick={() => act(() => pauseReceiveTransfer(transfer.id))}
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
						size="sm"
						className="flex-1"
						onClick={() => act(() => resumeReceiveTransfer(transfer.id))}
					>
						<Play className="size-4" />
						Resume
					</Button>
				)}
				{transfer.status === "complete" ? (
					<Button
						variant="outline"
						size="sm"
						className="flex-1"
						onClick={onRemoved}
					>
						Dismiss
					</Button>
				) : (
					<Button
						variant="outline"
						size="sm"
						className="flex-1"
						onClick={handleCancel}
					>
						<X className="size-4" />
						Cancel
					</Button>
				)}
			</div>

			{transfer.error && (
				<p className="text-xs text-destructive">{transfer.error}</p>
			)}
		</div>
	);
}
