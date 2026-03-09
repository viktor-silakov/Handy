import type { CorrectionPair } from "@/bindings";

const normalizeDictionaryValue = (value: string) =>
  value.trim().replace(/\s+/g, " ");

export const normalizeCorrectionPair = (
  entry: CorrectionPair,
): CorrectionPair | null => {
  const wrong = normalizeDictionaryValue(entry.wrong);
  const correct = normalizeDictionaryValue(entry.correct);

  if (!wrong || !correct || wrong.toLowerCase() === correct.toLowerCase()) {
    return null;
  }

  return { wrong, correct };
};

export const normalizeCorrectionDictionary = (
  entries: CorrectionPair[],
): CorrectionPair[] => {
  const merged = new Map<string, CorrectionPair>();

  for (const entry of entries) {
    const normalized = normalizeCorrectionPair(entry);
    if (!normalized) {
      continue;
    }

    merged.set(normalized.wrong.toLowerCase(), normalized);
  }

  return Array.from(merged.values()).sort((a, b) =>
    a.wrong.localeCompare(b.wrong, undefined, { sensitivity: "base" }),
  );
};

export const upsertCorrectionPair = (
  entries: CorrectionPair[],
  entry: CorrectionPair,
): CorrectionPair[] => normalizeCorrectionDictionary([...entries, entry]);
