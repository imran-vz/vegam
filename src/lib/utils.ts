import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function formatFileSize(bytes: number): string {
	if (bytes === 0) return "0 Bytes";
	const k = 1024;
	const sizes = ["Bytes", "KB", "MB", "GB", "TB"];
	const i = Math.floor(Math.log(bytes) / Math.log(k));
	return `${Math.round((bytes / k ** i) * 100) / 100} ${sizes[i]}`;
}

export function formatTransferSpeed(bytesPerSecond: number): string {
	return `${formatFileSize(bytesPerSecond)}/s`;
}

export function calculateProgress(transferred: number, total: number): number {
	if (total === 0) return 0;
	return Math.round((transferred / total) * 100);
}

export function truncateString(str: string, maxLength: number): string {
	if (str.length <= maxLength) return str;
	return `${str.slice(0, maxLength - 3)}...`;
}
