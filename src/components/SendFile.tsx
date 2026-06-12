import { open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Check, Copy, File, Loader2, Pause, Play, X } from "lucide-react";
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
	cancelSendTransfer,
	createSendTransfer,
	isApiError,
	listenToSendTransferUpdates,
	pauseSendTransfer,
	resumeSendTransfer,
	type SendTransferInfo,
} from "@/lib/api";
import { formatFileSize, parseError } from "@/lib/utils";

const STATUS_LABELS: Record<SendTransferInfo["status"], string> = {
	importing: "Preparing",
	available: "Available",
	paused: "Paused",
	expired: "Ticket expired",
	contentSuspect: "Checking file…",
	contentChanged: "File changed",
	sourceMissing: "File missing",
	cancelled: "Cancelled",
};

interface SendFileProps {
	initialTransfers: SendTransferInfo[];
}

export function SendFile({ initialTransfers }: SendFileProps) {
	const [transfer, setTransfer] = useState<SendTransferInfo | null>(
		initialTransfers[0] ?? null,
	);
	const [error, setError] = useState<string | null>(null);
	const [copied, setCopied] = useState(false);
	const [busy, setBusy] = useState(false);

	useEffect(() => {
		const unlisten = listenToSendTransferUpdates((updated) => {
			setTransfer((current) => {
				if (current === null || current.id === updated.id) {
					return updated.status === "cancelled" ? null : updated;
				}
				return current;
			});
		});
		return () => {
			unlisten.then((fn) => fn());
		};
	}, []);

	const handleSelectFile = async () => {
		setError(null);
		setBusy(true);
		try {
			const selected = await open({ multiple: false, directory: false });
			if (!selected) return;
			const info = await createSendTransfer(selected);
			setTransfer(info);
		} catch (err) {
			setError(isApiError(err) ? err.message : parseError(err));
		} finally {
			setBusy(false);
		}
	};

	const handleCopyTicket = async () => {
		if (!transfer?.ticket) return;
		await writeText(transfer.ticket);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
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
		await withTransfer(cancelSendTransfer);
		setTransfer(null);
	};

	return (
		<Card>
			<CardHeader>
				<CardTitle>Send File</CardTitle>
				<CardDescription>
					Select a file to create a Transfer Ticket
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				{transfer === null ? (
					<Button
						onClick={handleSelectFile}
						disabled={busy}
						className="w-full"
					>
						{busy ? (
							<Loader2 className="size-4 animate-spin" />
						) : (
							<File className="size-4" />
						)}
						Select File
					</Button>
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

						{transfer.status === "importing" && (
							<div className="space-y-2">
								<div className="flex justify-between text-sm">
									<span>Preparing…</span>
									<span>
										{Math.round((transfer.import_progress ?? 0) * 100)}%
									</span>
								</div>
								<Progress value={(transfer.import_progress ?? 0) * 100} />
							</div>
						)}

						{transfer.ticket && (
							<div className="space-y-2">
								<label
									htmlFor="transfer-ticket"
									className="text-sm font-medium"
								>
									Transfer Ticket
								</label>
								<div className="flex gap-2">
									<div
										id="transfer-ticket"
										className="flex-1 p-2 bg-muted rounded text-xs font-mono break-all"
									>
										{transfer.ticket.slice(0, 80)}...
									</div>
									<Button
										size="icon"
										variant="outline"
										onClick={handleCopyTicket}
									>
										{copied ? (
											<Check className="size-4" />
										) : (
											<Copy className="size-4" />
										)}
									</Button>
								</div>
								<p className="text-xs text-muted-foreground">
									Anyone with this ticket can download the file while Vegam
									stays open. It expires{" "}
									{transfer.expires_at_ms
										? new Date(transfer.expires_at_ms).toLocaleString()
										: "in 24 hours"}
									.
								</p>
								{transfer.active_receiver_count > 0 && (
									<p className="text-xs text-muted-foreground">
										{transfer.active_receiver_count} active{" "}
										{transfer.active_receiver_count === 1
											? "receiver"
											: "receivers"}
									</p>
								)}
							</div>
						)}

						<div className="flex gap-2">
							{transfer.status === "available" && (
								<Button
									variant="outline"
									className="flex-1"
									onClick={() => withTransfer(pauseSendTransfer)}
								>
									<Pause className="size-4" />
									Pause
								</Button>
							)}
							{transfer.status === "paused" && (
								<Button
									variant="outline"
									className="flex-1"
									onClick={() => withTransfer(resumeSendTransfer)}
								>
									<Play className="size-4" />
									Resume
								</Button>
							)}
							<Button
								variant="outline"
								className="flex-1"
								onClick={handleCancel}
							>
								<X className="size-4" />
								Cancel
							</Button>
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
