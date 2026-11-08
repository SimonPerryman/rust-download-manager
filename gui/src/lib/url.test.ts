import { describe, expect, it } from "vitest";
import {
  dragMayCarryUrl,
  extractHttpUrl,
  isHttpUrl,
  linksFromTransfer,
  urlFromTransfer,
} from "./url";

describe("extractHttpUrl", () => {
  it("returns a trimmed bare URL", () => {
    expect(extractHttpUrl("  https://example.com/file.iso \n")).toBe(
      "https://example.com/file.iso",
    );
  });

  it("picks the first URL from a uri-list, skipping # comments and blanks", () => {
    const list = "# dragged from browser\r\n\r\nhttp://a.example/x.zip\r\nhttps://b.example/y.zip";
    expect(extractHttpUrl(list)).toBe("http://a.example/x.zip");
  });

  it("skips non-http lines in favour of a later http one", () => {
    expect(extractHttpUrl("file:///C:/tmp/a.txt\nhttps://ok.example/a")).toBe(
      "https://ok.example/a",
    );
  });

  it("rejects non-http schemes, prose, and empty input", () => {
    expect(extractHttpUrl("ftp://example.com/a")).toBeNull();
    expect(extractHttpUrl("javascript:alert(1)")).toBeNull();
    expect(extractHttpUrl("see https://example.com for details")).toBeNull();
    expect(extractHttpUrl("just some text")).toBeNull();
    expect(extractHttpUrl("")).toBeNull();
    expect(extractHttpUrl(null)).toBeNull();
    expect(extractHttpUrl(undefined)).toBeNull();
  });
});

describe("isHttpUrl", () => {
  it("requires an http(s) scheme and a host", () => {
    expect(isHttpUrl("HTTPS://Example.com/a")).toBe(true);
    expect(isHttpUrl("http://localhost:8080/f")).toBe(true);
    expect(isHttpUrl("https://")).toBe(false);
    expect(isHttpUrl("example.com/a")).toBe(false);
  });
});

describe("urlFromTransfer", () => {
  const transfer = (data: Record<string, string>) => ({
    getData: (type: string) => data[type] ?? "",
  });

  it("prefers text/uri-list over text/plain", () => {
    const dt = transfer({
      "text/uri-list": "https://list.example/a",
      "text/plain": "https://plain.example/b",
    });
    expect(urlFromTransfer(dt)).toBe("https://list.example/a");
  });

  it("falls back to text/plain", () => {
    expect(urlFromTransfer(transfer({ "text/plain": "https://plain.example/b" }))).toBe(
      "https://plain.example/b",
    );
  });

  it("returns null when nothing usable is present", () => {
    expect(urlFromTransfer(transfer({ "text/plain": "hello" }))).toBeNull();
    expect(urlFromTransfer(null)).toBeNull();
  });
});

describe("dragMayCarryUrl", () => {
  it("is true for text drags and false for file-only drags", () => {
    expect(dragMayCarryUrl(["text/uri-list", "text/html"])).toBe(true);
    expect(dragMayCarryUrl(["text/plain"])).toBe(true);
    expect(dragMayCarryUrl(["Files"])).toBe(false);
  });
});

describe("linksFromTransfer", () => {
  const transfer = (data: Record<string, string>) => ({
    getData: (type: string) => data[type] ?? "",
  });

  it("hands over every pasted line when any holds a link", () => {
    const text = ["https://a.example/x", "not a link", "https://b.example/y", ""].join("\n");
    expect(linksFromTransfer(transfer({ "text/plain": text }))).toBe(
      ["https://a.example/x", "not a link", "https://b.example/y"].join("\n"),
    );
  });

  it("takes just the links from a uri-list", () => {
    const list = ["# comment", "https://a.example/x", "https://b.example/y"].join("\r\n");
    expect(linksFromTransfer(transfer({ "text/uri-list": list, "text/plain": "ignored" }))).toBe(
      ["https://a.example/x", "https://b.example/y"].join("\n"),
    );
  });

  it("ignores prose with a link in it, as the dialog would", () => {
    expect(linksFromTransfer(transfer({ "text/plain": "get https://a.example/x now" }))).toBeNull();
  });

  it("takes a line of several space-separated links", () => {
    const text = "https://a.example/x https://b.example/y";
    expect(linksFromTransfer(transfer({ "text/plain": text }))).toBe(text);
  });

  it("returns null with no link at all", () => {
    expect(linksFromTransfer(transfer({ "text/plain": "hello there" }))).toBeNull();
    expect(linksFromTransfer(transfer({}))).toBeNull();
    expect(linksFromTransfer(null)).toBeNull();
  });
});
