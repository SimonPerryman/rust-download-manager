import { describe, expect, it } from "vitest";
import { describeError } from "./errors";

describe("describeError", () => {
  it("explains a refused request, naming the code", () => {
    expect(describeError("server returned HTTP 403")).toBe(
      "The server refused the request (403 Forbidden). The link may have expired. Copy a fresh link and retry.",
    );
    expect(describeError("server returned HTTP 401")).toBe(
      "The server refused the request (401 Unauthorized). The link may have expired. Copy a fresh link and retry.",
    );
  });

  it("explains a missing file", () => {
    expect(describeError("server returned HTTP 404")).toBe(
      "The file isn't at this link any more (404 Not Found). Check the link and retry.",
    );
    expect(describeError("HTTP 410")).toBe(
      "The file isn't at this link any more (410 Gone). Check the link and retry.",
    );
  });

  it("explains rate limiting", () => {
    expect(describeError("server returned HTTP 429")).toBe(
      "The server is limiting requests (429 Too Many Requests). Wait a minute, then retry.",
    );
  });

  it("explains server errors, with or without a known reason phrase", () => {
    expect(describeError("server returned HTTP 503")).toBe(
      "The server had a problem (503 Service Unavailable). Retry in a moment.",
    );
    expect(describeError("server returned HTTP 507")).toBe(
      "The server had a problem (507). Retry in a moment.",
    );
  });

  it("explains any other code generically", () => {
    expect(describeError("server returned HTTP 418")).toBe(
      "The server answered with an error (418). Check the link and retry.",
    );
  });

  it("explains timeouts, DNS, connection and disk-full errors", () => {
    expect(describeError("operation timed out")).toBe(
      "The server stopped responding. Check your connection, then retry.",
    );
    expect(describeError("read timeout")).toBe(
      "The server stopped responding. Check your connection, then retry.",
    );
    expect(describeError("error trying to connect: dns error: failed to lookup address")).toBe(
      "Couldn't find the server. Check the link, then retry.",
    );
    expect(describeError("Connection refused (os error 111)")).toBe(
      "Couldn't reach the server. Check your connection, then retry.",
    );
    expect(describeError("connection reset by peer")).toBe(
      "Couldn't reach the server. Check your connection, then retry.",
    );
    expect(describeError("No space left on device (os error 28)")).toBe(
      "The disk is full. Free some space, then retry.",
    );
    expect(describeError("There is not enough space on the disk. (os error 112)")).toBe(
      "The disk is full. Free some space, then retry.",
    );
  });

  it("passes unknown text through as a sentence", () => {
    expect(describeError("checksum mismatch")).toBe("Checksum mismatch. Retry, or check the link.");
    expect(describeError("Already a sentence.")).toBe(
      "Already a sentence. Retry, or check the link.",
    );
  });

  it("has a default for no error text", () => {
    expect(describeError(null)).toBe("The download failed. Retry, or check the link.");
    expect(describeError("  ")).toBe("The download failed. Retry, or check the link.");
  });

  it("ignores words in the URL that reqwest embeds", () => {
    expect(
      describeError(
        "request failed: error decoding response body for url (https://connect.example.org/dns-tools.zip)",
      ),
    ).toBe("Request failed: error decoding response body. Retry, or check the link.");
    expect(
      describeError(
        "request failed: error sending request for url (https://example.com/a.zip): connection refused",
      ),
    ).toBe("Couldn't reach the server. Check your connection, then retry.");
    expect(
      describeError(
        "request failed: error decoding response body for url (https://h.example/a(b)/connect-tools.zip)",
      ),
    ).toBe("Request failed: error decoding response body. Retry, or check the link.");
  });
});
