import { useEffect, useState } from "react";
import { PartialDownloads } from "@/components/PartialDownloads";
import { ReceiveFile } from "@/components/ReceiveFile";
import { SendFile } from "@/components/SendFile";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { type AppSnapshot, initApp } from "@/lib/api";

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
					<p className="text-xs md:text-sm text-muted-foreground">
						{snapshot.settings.display_name}
					</p>
				</div>

				<Tabs defaultValue="send" className="w-full">
					<TabsList className="grid w-full grid-cols-3">
						<TabsTrigger value="send">Send</TabsTrigger>
						<TabsTrigger value="receive">Receive</TabsTrigger>
						<TabsTrigger value="storage">Storage</TabsTrigger>
					</TabsList>
					<TabsContent value="send" className="mt-4 md:mt-6">
						<SendFile initialTransfers={snapshot.send_transfers} />
					</TabsContent>
					<TabsContent value="receive" className="mt-4 md:mt-6">
						<ReceiveFile initialTransfers={snapshot.receive_transfers} />
					</TabsContent>
					<TabsContent value="storage" className="mt-4 md:mt-6">
						<PartialDownloads />
					</TabsContent>
				</Tabs>
			</div>
		</div>
	);
}

export default App;
