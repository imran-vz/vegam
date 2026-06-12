import { open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
	Check,
	Copy,
	File,
	FileSearch,
	Loader2,
	Pause,
	Play,
	X,
} from "lucide-react";
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
	reselectSendSource,
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
	const [transfers, setTransfers] = useState<SendTransferInfo[]>(
		initialTransfers,
	);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);

	useEffect(() => {
		const unlisten = listenToSendTransferUpdates((updated) => {
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
			setTransfers((current) => [
				...current.filter((t) => t.id !== info.id),
				info,
			]);
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
				<CardTitle>Send File</CardTitle>
				<CardDescription>
					Select a file to create a Transfer Ticket
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				<Button onClick={handleSelectFile} disabled={busy} className="w-full">
					{busy ? (
						<Loader2 className="size-4 animate-spin" />
					) : (
						<File className="size-4" />
					)}
					Select File
				</Button>

				{transfers.map((transfer) => (
					<SendTransferCard
						key={transfer.id}
						transfer={transfer}
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

function SendTransferCard({
	transfer,
	onRemoved,
	onError,
}: {
	transfer: SendTransferInfo;
	onRemoved: () => void;
	onError: (message: string | null) => void;
}) {
	const [copied, setCopied] = useState(false);

	const act = async (action: () => Promise<unknown>) => {
		onError(null);
		try {
			await action();
		} catch (err) {
			onError(isApiError(err) ? err.message : parseError(err));
		}
	};

	const handleCopyTicket = async () => {
		if (!transfer.ticket) return;
		await writeText(transfer.ticket);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
	};

	const handleReselect = async () => {
		await act(async () => {
			const selected = await open({ multiple: false, directory: false });
			if (!selected) return;
			await reselectSendSource(transfer.id, selected);
		});
	};

	const handleCancel = async () => {
		await act(() => cancelSendTransfer(transfer.id));
		onRemoved();
	};

	const needsReselect =
		transfer.status === "sourceMissing" || transfer.status === "contentChanged";

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

			{transfer.status === "importing" && (
				<div className="space-y-2">
					<div className="flex justify-between text-sm">
						<span>Preparing…</span>
						<span>{Math.round((transfer.import_progress ?? 0) * 100)}%</span>
					</div>
					<Progress value={(transfer.import_progress ?? 0) * 100} />
				</div>
			)}

			{transfer.ticket && transfer.status !== "contentChanged" && (
				<div className="space-y-2">
					<div className="flex gap-2">
						<div className="flex-1 p-2 bg-muted rounded text-xs font-mono break-all">
							{transfer.ticket.slice(0, 64)}...
						</div>
						<Button size="icon" variant="outline" onClick={handleCopyTicket}>
							{copied ? (
								<Check className="size-4" />
							) : (
								<Copy className="size-4" />
							)}
						</Button>
					</div>
					<p className="text-xs text-muted-foreground">
						Anyone with this Transfer Ticket can download the file while you
						keep Vegam open. It expires{" "}
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

			{transfer.status === "contentChanged" && (
				<p className="text-xs text-muted-foreground">
					The file's content changed, so this Transfer Ticket no longer
					works. Select the file again to create a new ticket.
				</p>
			)}

			<div className="flex gap-2">
				{transfer.status === "available" && (
					<Button
						variant="outline"
						size="sm"
						className="flex-1"
						onClick={() => act(() => pauseSendTransfer(transfer.id))}
					>
						<Pause className="size-4" />
						Pause
					</Button>
				)}
				{transfer.status === "paused" && (
					<Button
						variant="outline"
						size="sm"
						className="flex-1"
						onClick={() => act(() => resumeSendTransfer(transfer.id))}
					>
						<Play className="size-4" />
						Resume
					</Button>
				)}
				{needsReselect && (
					<Button
						variant="outline"
						size="sm"
						className="flex-1"
						onClick={handleReselect}
					>
						<FileSearch className="size-4" />
						Locate File
					</Button>
				)}
				<Button
					variant="outline"
					size="sm"
					className="flex-1"
					onClick={handleCancel}
				>
					<X className="size-4" />
					Cancel
				</Button>
			</div>

			{transfer.error && (
				<p className="text-xs text-destructive">{transfer.error}</p>
			)}
		</div>
	);
}
