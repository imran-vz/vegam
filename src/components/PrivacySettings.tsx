import { save } from "@tauri-apps/plugin-dialog";
import { FileDown } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import {
	exportDiagnostics,
	isApiError,
	setAnalyticsEnabled,
	type Settings,
} from "@/lib/api";
import { parseError } from "@/lib/utils";

export function PrivacySettings({
	initialSettings,
}: {
	initialSettings: Settings;
}) {
	const [analytics, setAnalytics] = useState(
		initialSettings.analytics_enabled,
	);
	const [message, setMessage] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);

	const handleToggleAnalytics = async (enabled: boolean) => {
		setError(null);
		try {
			const settings = await setAnalyticsEnabled(enabled);
			setAnalytics(settings.analytics_enabled);
		} catch (err) {
			setError(isApiError(err) ? err.message : parseError(err));
		}
	};

	const handleExportDiagnostics = async () => {
		setError(null);
		setMessage(null);
		try {
			const destination = await save({
				defaultPath: "vegam-diagnostics.log",
			});
			if (!destination) return;
			await exportDiagnostics(destination);
			setMessage("Diagnostics exported.");
		} catch (err) {
			setError(isApiError(err) ? err.message : parseError(err));
		}
	};

	return (
		<Card>
			<CardHeader>
				<CardTitle>Privacy & Diagnostics</CardTitle>
				<CardDescription>
					Local logs never include file names, tickets, or addresses.
					Nothing leaves this device unless you act here.
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				<label className="flex items-center justify-between gap-4 cursor-pointer">
					<div>
						<p className="text-sm font-medium">Share anonymous usage data</p>
						<p className="text-xs text-muted-foreground">
							Off by default. When on, only product events and coarse size
							ranges are sent — never file names, tickets, or peers.
						</p>
					</div>
					<input
						type="checkbox"
						checked={analytics}
						onChange={(e) => handleToggleAnalytics(e.target.checked)}
						className="size-4 accent-primary"
					/>
				</label>

				<div className="flex items-center justify-between gap-4">
					<div>
						<p className="text-sm font-medium">Export diagnostics</p>
						<p className="text-xs text-muted-foreground">
							Save the local log file to share with support.
						</p>
					</div>
					<Button variant="outline" size="sm" onClick={handleExportDiagnostics}>
						<FileDown className="size-4" />
						Export
					</Button>
				</div>

				{message && <p className="text-xs text-muted-foreground">{message}</p>}
				{error && (
					<div className="p-3 text-sm text-destructive bg-destructive/10 rounded-lg">
						{error}
					</div>
				)}
			</CardContent>
		</Card>
	);
}
