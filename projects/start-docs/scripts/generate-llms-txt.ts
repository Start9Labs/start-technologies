export {};

/**
 * Generate llms.txt from multiple mdBook instances
 *
 * Discovers books by scanning for {name}/book.toml at repo root,
 * parses each book's SUMMARY.md to get page hierarchy,
 * reads each page's H1 title and intro prose,
 * and outputs per-book files (docs/<book>/llms.txt, docs/<book>/llms-full.txt)
 * plus global combined files (docs/llms.txt, docs/llms-full.txt).
 *
 * Usage: npx tsx generate-llms-txt.ts
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "fs";
import { join } from "path";
import { globSync } from "glob";

const ROOT_DIR = join(import.meta.dirname, "..");
const OUT_DIR = join(ROOT_DIR, "docs");
const BASE_URL = "https://docs.start9.com";

type Book = {
  name: string;
  label: string;
  description: string;
  srcDir: string;
  summaryPath: string;
};

type SummaryItem =
  | { kind: "part"; title: string }
  | { kind: "page"; title: string; path: string; indent: number };

/** Book metadata: display labels and one-line descriptions */
const bookInfo: Record<string, { label: string; description: string }> = {
  "start-os": {
    label: "StartOS",
    description:
      "Documentation for StartOS, a Linux-based operating system for self-hosting services on a personal server.",
  },
  "start-tunnel": {
    label: "StartTunnel",
    description:
      "A virtual private router (VPR) — a minimal, self-hosted router that runs on a VPS. Use it for private remote access to self-hosted services, or to expose services to the public Internet.",
  },
  packaging: {
    label: "Service Packaging",
    description:
      "Learn how to package self-hosted services for StartOS using the Start SDK.",
  },
  "bitcoin-guides": {
    label: "Bitcoin Guides",
    description:
      "Guides for running Bitcoin, Lightning and related services on StartOS.",
  },
  "start-wrt": {
    label: "StartWRT",
    description:
      "A router operating system for home-based self-hosting, built on OpenWrt. Runs on the SpaceMiT K1 and provides WiFi, VPN, DDNS, and secure remote access to services on your LAN.",
  },
};

/** Discover all books by finding {name}/book.toml at repo root */
function discoverBooks(): Book[] {
  const bookTomls = globSync("*/book.toml", { cwd: ROOT_DIR });
  return bookTomls.map((tomlPath) => {
    const name = tomlPath.split("/")[0];
    const info = bookInfo[name] || { label: name, description: "" };
    return {
      name,
      label: info.label,
      description: info.description,
      srcDir: join(ROOT_DIR, name, "src"),
      summaryPath: join(ROOT_DIR, name, "src", "SUMMARY.md"),
    };
  });
}

// Parse SUMMARY.md into a flat list of part titles and page entries.
// Part titles are `# Title` lines (after the first `# Summary` heading).
// Page entries are `- [Title](path.md)` lines with indentation levels.
function parseSummary(summaryPath: string): SummaryItem[] {
  const content = readFileSync(summaryPath, "utf-8");
  const items: SummaryItem[] = [];
  let seenFirstHeading = false;

  for (const line of content.split("\n")) {
    // Match part title lines like "# Services" (skip the first "# Summary")
    const partMatch = line.match(/^#\s+(.+)$/);
    if (partMatch) {
      if (!seenFirstHeading) {
        seenFirstHeading = true;
        continue; // Skip "# Summary"
      }
      items.push({ kind: "part", title: partMatch[1].trim() });
      continue;
    }

    // Match lines like "- [Title](path.md)" or "  - [Title](path.md)"
    const pageMatch = line.match(/^(\s*)-\s+\[([^\]]+)\]\(([^)]+)\)/);
    if (!pageMatch) continue;

    const indent = pageMatch[1].length / 2; // 2-space indentation
    const title = pageMatch[2];
    const path = pageMatch[3];

    items.push({ kind: "page", title, path, indent });
  }

  return items;
}

// Read a markdown file and extract its H1 title
function readPage(
  srcDir: string,
  relPath: string,
): { title: string; content: string } | null {
  const filePath = join(srcDir, relPath);
  if (!existsSync(filePath)) {
    console.warn(`  Warning: ${relPath} not found`);
    return null;
  }

  try {
    const content = readFileSync(filePath, "utf-8");
    const titleMatch = content.match(/^#\s+(.+)$/m);
    const title = titleMatch ? titleMatch[1].trim() : "";
    return { title, content };
  } catch {
    return null;
  }
}

// Extract H2 headings from markdown content
function extractHeadings(content: string): string[] {
  const headings: string[] = [];
  for (const line of content.split("\n")) {
    const match = line.match(/^##\s+(.+)$/);
    if (match) headings.push(match[1].trim());
  }
  return headings;
}

// Extract introductory prose: text between the H1 heading and the first H2.
// Strips markdown links, images, admonitions, code blocks, and HTML tags
// to produce clean plaintext suitable for an llms.txt summary.
function extractIntro(content: string): string {
  const lines = content.split("\n");

  // Find start (line after H1) and end (first H2 or EOF)
  let start = -1;
  let end = lines.length;
  for (let i = 0; i < lines.length; i++) {
    if (start === -1 && /^#\s+/.test(lines[i])) {
      start = i + 1;
      continue;
    }
    if (start !== -1 && /^##\s+/.test(lines[i])) {
      end = i;
      break;
    }
  }

  if (start === -1) return "";

  const intro = lines
    .slice(start, end)
    .filter((line) => {
      // Drop code fences, admonitions, images, HTML tags, and blank lines
      if (/^```/.test(line)) return false;
      if (/^>\s*\[!/.test(line)) return false;
      if (/^!\[/.test(line)) return false;
      if (/^</.test(line)) return false;
      return true;
    })
    .map((line) =>
      line
        // Convert markdown links [text](url) to just text
        .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
        // Strip bold/italic markers
        .replace(/\*{1,2}([^*]+)\*{1,2}/g, "$1")
        .replace(/_{1,2}([^_]+)_{1,2}/g, "$1")
        // Strip inline code backticks
        .replace(/`([^`]+)`/g, "$1")
        .trim(),
    )
    .filter(Boolean)
    .join(" ");

  return intro;
}

// Convert a markdown path to a URL for a specific book
function pathToUrl(bookName: string, relPath: string): string {
  // README.md -> index.html, foo.md -> foo.html
  const htmlPath = relPath
    .replace(/README\.md$/, "index.html")
    .replace(/\.md$/, ".html");
  return `${BASE_URL}/${bookName}/${htmlPath}`;
}

// Render a page entry for llms.txt: title link, intro prose, sections
function renderPageEntry(
  book: Book,
  entry: Extract<SummaryItem, { kind: "page" }>,
  headingLevel: string,
): string[] {
  const page = readPage(book.srcDir, entry.path);
  const title = page?.title || entry.title;
  const url = pathToUrl(book.name, entry.path);
  const headings = page ? extractHeadings(page.content) : [];
  const intro = page ? extractIntro(page.content) : "";

  const lines: string[] = [];
  lines.push(`${headingLevel} [${title}](${url})`);
  if (intro) {
    lines.push(intro);
  }
  if (headings.length > 0) {
    lines.push(`Sections: ${headings.join(", ")}`);
  }
  lines.push("");
  return lines;
}

// Generate llms.txt content for a single book
// Lists every page with its intro prose and section headings
// so an AI can decide which pages to fetch for full content.
function generateBookLlmsTxt(book: Book): string {
  const entries = parseSummary(book.summaryPath);
  const lines: string[] = [`# ${book.label}`, ""];

  if (book.description) {
    lines.push(`> ${book.description}`);
    lines.push("");
  }

  lines.push(`Full content: ${BASE_URL}/${book.name}/llms-full.txt`);
  lines.push("");

  for (const item of entries) {
    if (item.kind === "part") {
      lines.push(`## ${item.title}`);
      lines.push("");
    } else {
      lines.push(...renderPageEntry(book, item, "###"));
    }
  }

  return lines.join("\n");
}

// Generate llms-full.txt content for a single book
function generateBookLlmsFullTxt(book: Book): string {
  const entries = parseSummary(book.summaryPath);
  const parts: string[] = [`# ${book.label} — Full Content`, ""];

  if (book.description) {
    parts.push(`> ${book.description}`);
    parts.push("");
  }

  for (const item of entries) {
    if (item.kind === "part") {
      parts.push(`---`);
      parts.push(`## ${item.title}`);
      parts.push("");
      continue;
    }

    const page = readPage(book.srcDir, item.path);
    if (!page) continue;

    const title = page.title || item.title;
    const body = page.content.trim();

    if (!body) continue;

    parts.push(`---`);
    parts.push(`### Page: ${title}`);
    parts.push("");
    parts.push(body);
    parts.push("");
  }

  return parts.join("\n");
}

// Generate combined llms.txt across all books
// Uses the same detailed format as per-book llms.txt, grouped by book.
function generateGlobalLlmsTxt(books: Book[]): string {
  const lines: string[] = [
    "# Start9 Documentation",
    "",
    `> Documentation for Start9 products including ${books.map((b) => b.label).join(", ")}.`,
    "",
    "Per-book indexes:",
  ];

  for (const book of books) {
    lines.push(`- [${book.label}](${BASE_URL}/${book.name}/llms.txt)`);
  }

  lines.push("");
  lines.push(`Full content: ${BASE_URL}/llms-full.txt`);
  lines.push("");

  for (const book of books) {
    if (!existsSync(book.summaryPath)) continue;

    const entries = parseSummary(book.summaryPath);
    lines.push(`## ${book.label}`);
    lines.push("");

    for (const item of entries) {
      if (item.kind === "part") {
        lines.push(`### ${item.title}`);
        lines.push("");
      } else {
        lines.push(...renderPageEntry(book, item, "####"));
      }
    }
  }

  return lines.join("\n");
}

// Generate combined llms-full.txt across all books
function generateGlobalLlmsFullTxt(books: Book[]): string {
  const parts: string[] = [
    "# Start9 Documentation — Full Content",
    "",
    `> Complete documentation for Start9 products including ${books.map((b) => b.label).join(", ")}.`,
    "",
  ];

  for (const book of books) {
    if (!existsSync(book.summaryPath)) continue;

    const entries = parseSummary(book.summaryPath);
    parts.push(`---`);
    parts.push(`# ${book.label}`);
    parts.push("");

    for (const item of entries) {
      if (item.kind === "part") {
        parts.push(`---`);
        parts.push(`## ${item.title}`);
        parts.push("");
        continue;
      }

      const page = readPage(book.srcDir, item.path);
      if (!page) continue;

      const title = page.title || item.title;
      const body = page.content.trim();

      if (!body) continue;

      parts.push(`---`);
      parts.push(`### Page: ${title}`);
      parts.push("");
      parts.push(body);
      parts.push("");
    }
  }

  return parts.join("\n");
}

function main() {
  console.log("Generating llms.txt...");

  const books = discoverBooks();
  console.log(
    `  Found ${books.length} book(s): ${books.map((b) => b.name).join(", ")}`,
  );

  // Ensure output directory exists
  if (!existsSync(OUT_DIR)) {
    mkdirSync(OUT_DIR, { recursive: true });
  }

  // Per-book llms.txt and llms-full.txt
  for (const book of books) {
    if (!existsSync(book.summaryPath)) continue;

    const bookOutDir = join(OUT_DIR, book.name);
    if (!existsSync(bookOutDir)) {
      mkdirSync(bookOutDir, { recursive: true });
    }

    const bookLlmsTxt = generateBookLlmsTxt(book);
    const bookLlmsTxtPath = join(bookOutDir, "llms.txt");
    writeFileSync(bookLlmsTxtPath, bookLlmsTxt);
    console.log(`  Wrote ${bookLlmsTxtPath}`);

    const bookLlmsFullTxt = generateBookLlmsFullTxt(book);
    const bookLlmsFullPath = join(bookOutDir, "llms-full.txt");
    writeFileSync(bookLlmsFullPath, bookLlmsFullTxt);
    console.log(
      `  Wrote ${bookLlmsFullPath} (${(bookLlmsFullTxt.length / 1024).toFixed(0)}KB)`,
    );
  }

  // Global combined llms.txt and llms-full.txt
  const llmsTxt = generateGlobalLlmsTxt(books);
  const llmsTxtPath = join(OUT_DIR, "llms.txt");
  writeFileSync(llmsTxtPath, llmsTxt);
  console.log(`  Wrote ${llmsTxtPath}`);

  const llmsFullTxt = generateGlobalLlmsFullTxt(books);
  const llmsFullPath = join(OUT_DIR, "llms-full.txt");
  writeFileSync(llmsFullPath, llmsFullTxt);
  console.log(
    `  Wrote ${llmsFullPath} (${(llmsFullTxt.length / 1024).toFixed(0)}KB)`,
  );

  console.log("Done!");
}

main();
