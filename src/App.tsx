import { Check, Pencil } from "lucide-react";
import { useEffect, useState } from "react";
import { PartialDownloads } from "@/components/PartialDownloads";
import { PrivacySettings } from "@/components/PrivacySettings";
import { ReceiveFile } from "@/components/ReceiveFile";
import { SendFile } from "@/components/SendFile";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { type AppSnapshot, initApp, setDisplayName } from "@/lib/api";

/** Cosmetic Device name (ADR 0017): editable, never identity or trust. */
function DisplayNameEditor({ initialName }: { initialName: string }) {
	const [name, setName] = useState(initialName);
	const [draft, setDraft] = useState(initialName);
	const [editing, setEditing] = useState(false);
	const [saveError, setSaveError] = useState<string | null>(null);

	const commit = async () => {
		const trimmed = draft.trim();
		if (trimmed && trimmed !== name) {
			try {
				const settings = await setDisplayName(trimmed);
				setName(settings.display_name);
				setDraft(settings.display_name);
				setSaveError(null);
			} catch {
				// Keep the editor open with the draft so the edit isn't
				// silently discarded.
				setSaveError("Could not save the name. Try again.");
				return;
			}
		} else {
			setDraft(name);
		}
		setEditing(false);
	};

	if (!editing) {
		return (
			<button
				type="button"
				className="inline-flex items-center gap-1 text-xs md:text-sm text-muted-foreground hover:text-foreground"
				onClick={() => setEditing(true)}
				title="Edit this device's display name"
			>
				{name}
				<Pencil className="size-3" />
			</button>
		);
	}
	return (
		<span className="inline-flex flex-col items-center gap-1">
			<span className="inline-flex items-center gap-1">
				<input
					value={draft}
					onChange={(e) => setDraft(e.target.value)}
					onKeyDown={(e) => {
						if (e.key === "Enter") commit();
						if (e.key === "Escape") {
							setDraft(name);
							setSaveError(null);
							setEditing(false);
						}
					}}
					className="text-xs md:text-sm border rounded px-2 py-0.5 bg-background"
					maxLength={64}
				/>
				<Button
					size="icon"
					variant="ghost"
					className="size-6"
					onClick={commit}
				>
					<Check className="size-3" />
				</Button>
			</span>
			{saveError && <span className="text-xs text-destructive">{saveError}</span>}
		</span>
	);
}

function App() {
	const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
	const [initError, setInitError] = useState<string | null>(null);

	useEffect(() => {
		const initialize = async () => {
			try {
				const snap = await initApp();
				setSnapshot(snap);
			} catch (error) {
				console.error("Failed to initialize:", error);
				setInitError(
					error instanceof Error ? error.message : String(error),
				);
			}
		};
		initialize();
	}, []);

	if (initError) {
		return (
			<div className="flex items-center justify-center min-h-screen bg-background">
				<div className="text-center space-y-2 max-w-md">
					<p className="text-sm text-destructive">
						Failed to start: {initError}
					</p>
				</div>
			</div>
		);
	}

	if (snapshot === null) {
		return (
			<div className="flex items-center justify-center min-h-screen bg-background">
				<div className="text-center space-y-2">
					<div className="animate-spin size-8 border-4 border-primary border-t-transparent rounded-full mx-auto" />
					<p className="text-sm text-muted-foreground">Initializing...</p>
				</div>
			</div>
		);
	}

	return (
		<div className="min-h-screen bg-background p-4 md:p-6">
			<div className="max-w-2xl mx-auto space-y-4 md:space-y-6">
				<div className="text-center space-y-1">
					<div className="flex items-center justify-center gap-3">
						<h1 className="text-2xl md:text-3xl font-bold">Vegam</h1>
					</div>
					<DisplayNameEditor initialName={snapshot.settings.display_name} />
				</div>

				<Tabs defaultValue="send" className="w-full">
					<TabsList className="grid w-full grid-cols-3">
						<TabsTrigger value="send">Send</TabsTrigger>
						<TabsTrigger value="receive">Receive</TabsTrigger>
						<TabsTrigger value="storage">Storage</TabsTrigger>
					</TabsList>
					{/* forceMount keeps the tab panels mounted across switches:
					    the components hold live transfer state fed by events,
					    and unmounting would reset them to the stale app-start
					    snapshot (losing active transfer cards and showing a
					    wrong analytics toggle). */}
					<TabsContent
						forceMount
						value="send"
						className="mt-4 md:mt-6 data-[state=inactive]:hidden"
					>
						<SendFile initialTransfers={snapshot.send_transfers} />
					</TabsContent>
					<TabsContent
						forceMount
						value="receive"
						className="mt-4 md:mt-6 data-[state=inactive]:hidden"
					>
						<ReceiveFile initialTransfers={snapshot.receive_transfers} />
					</TabsContent>
					<TabsContent
						forceMount
						value="storage"
						className="mt-4 md:mt-6 space-y-4 data-[state=inactive]:hidden"
					>
						<PartialDownloads />
						<PrivacySettings initialSettings={snapshot.settings} />
					</TabsContent>
				</Tabs>
			</div>
		</div>
	);
}

export default App;
