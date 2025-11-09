import { open } from "@tauri-apps/plugin-dialog";
import { debug } from "@tauri-apps/plugin-log";
import { Check, Copy, File, Loader2 } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { type BlobTicketInfo, sendFile } from "@/lib/api";
import { formatFileSize, parseError } from "@/lib/utils";

const STEPS = {
	select: "Select File",
	selecting: "Selecting File",
	generating: "Generating Ticket",
	failed: "Failed",
} as const;

export function SendFile() {
	const [ticketInfo, setTicketInfo] = useState<BlobTicketInfo | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [copied, setCopied] = useState(false);
	const [step, setStep] = useState<keyof typeof STEPS>("select");

	const handleSelectFile = async () => {
		try {
			setIsLoading(true);
			setError(null);
			setStep("selecting");

			const selected = await open({
				multiple: false,
				directory: false,
			});

			if (!selected) {
				setStep("select");
				return;
			}

			debug(`selected file: ${selected}`);
			setStep("generating");
			const ticket = await sendFile(selected);
			setStep("select");
			setTicketInfo(ticket);
		} catch (err) {
			setError(parseError(err));
			setStep("failed");
		} finally {
			setIsLoading(false);
		}
	};

	const handleCopyTicket = async () => {
		if (!ticketInfo) return;

		await navigator.clipboard.writeText(ticketInfo.ticket);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
	};

	return (
		<Card>
			<CardHeader>
				<CardTitle>Send File</CardTitle>
				<CardDescription>
					Select a file to generate a transfer ticket
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				{!ticketInfo ? (
					<Button
						onClick={handleSelectFile}
						disabled={isLoading}
						className="w-full"
					>
						{isLoading ? (
							<Loader2 className="h-4 w-4 animate-spin" />
						) : (
							<File className="h-4 w-4" />
						)}

						{STEPS[step]}
					</Button>
				) : (
					<div className="space-y-4">
						<div className="flex items-center justify-between p-3 bg-muted rounded-lg">
							<div>
								<p className="font-medium">{ticketInfo.file_name}</p>
								<p className="text-sm text-muted-foreground">
									{formatFileSize(ticketInfo.file_size)}
								</p>
							</div>
							<Badge>Ready</Badge>
						</div>

						<div className="space-y-2">
							<label htmlFor="transfer-ticket" className="text-sm font-medium">
								Transfer Ticket
							</label>
							<div className="flex gap-2">
								<div
									id="transfer-ticket"
									className="flex-1 p-2 bg-muted rounded text-xs font-mono break-all"
								>
									{ticketInfo.ticket.slice(0, 80)}...
								</div>
								<Button
									size="icon"
									variant="outline"
									onClick={handleCopyTicket}
								>
									{copied ? (
										<Check className="h-4 w-4" />
									) : (
										<Copy className="h-4 w-4" />
									)}
								</Button>
							</div>
						</div>

						<Button
							variant="outline"
							onClick={() => setTicketInfo(null)}
							className="w-full"
						>
							Send Another File
						</Button>
					</div>
				)}

				{error && (
					<div className="p-3 text-sm text-destructive bg-destructive/10 rounded-lg">
						{error}
					</div>
				)}
			</CardContent>
		</Card>
	);
}
