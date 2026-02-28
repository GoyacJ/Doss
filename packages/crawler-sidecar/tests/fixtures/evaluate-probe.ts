import { extractCandidateCards, extractJobCards } from "../../src/adapters/base";

async function captureEvaluateSource(kind: "job" | "candidate") {
  let source = "";
  const page = {
    async evaluate(callback: unknown) {
      source = typeof callback === "function" ? callback.toString() : String(callback);
      return [];
    },
  };

  const selectors = {
    cards: [".card"],
    title: [".title"],
    name: [".name"],
    company: [".company"],
    link: ["a"],
  };

  if (kind === "job") {
    await extractJobCards(page as never, selectors);
  } else {
    await extractCandidateCards(page as never, selectors);
  }

  return source;
}

async function main() {
  const jobSource = await captureEvaluateSource("job");
  const candidateSource = await captureEvaluateSource("candidate");

  process.stdout.write(JSON.stringify({
    jobHasHelper: jobSource.includes("__name"),
    candidateHasHelper: candidateSource.includes("__name"),
  }));
}

main().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exit(1);
});
