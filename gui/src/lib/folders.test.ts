import { describe, expect, it } from "vitest";
import { deriveFolders, folderOf } from "./folders";
import { snap } from "./testSnapshot";

describe("folderOf", () => {
  it("takes the parent dir with either separator", () => {
    expect(folderOf("C:\\Users\\a\\Downloads\\x.iso")).toBe("C:\\Users\\a\\Downloads");
    expect(folderOf("/home/a/isos/x.iso")).toBe("/home/a/isos");
    expect(folderOf("C:/Users/a/x.iso")).toBe("C:/Users/a");
  });

  it("keeps the separator of a root", () => {
    expect(folderOf("C:\\x.iso")).toBe("C:\\");
    expect(folderOf("/x")).toBe("/");
  });
});

const at = (id: number, target: string) => snap({ id, target });

describe("deriveFolders", () => {
  it("groups by dir, labels by last component, counts each", () => {
    const ds = [at(1, "/home/a/isos/x.iso"), at(2, "/home/a/isos/y.iso"), at(3, "/tmp/z")];
    expect(deriveFolders(ds)).toEqual([
      { dir: "/home/a/isos", label: "isos", count: 2 },
      { dir: "/tmp", label: "tmp", count: 1 },
    ]);
  });

  it("labels a root as itself", () => {
    const ds = [at(1, "C:\\x.iso"), at(2, "/y")];
    expect(deriveFolders(ds).map((f) => f.label)).toEqual(["/", "C:\\"]);
  });

  it("gives duplicate labels the shortest distinguishing suffix", () => {
    const ds = [
      at(1, "/x/a/isos/1"),
      at(2, "/y/a/isos/2"),
      at(3, "/b/isos/3"),
      at(4, "C:\\c\\isos\\4"),
    ];
    const labels = Object.fromEntries(deriveFolders(ds).map((f) => [f.dir, f.label]));
    expect(labels).toEqual({
      "/x/a/isos": "x/a/isos",
      "/y/a/isos": "y/a/isos",
      "/b/isos": "b/isos",
      "C:\\c\\isos": "c\\isos",
    });
  });

  it("groups by exact string (no case folding)", () => {
    const ds = [at(1, "C:\\Dl\\a"), at(2, "c:\\dl\\b")];
    expect(deriveFolders(ds)).toHaveLength(2);
  });

  it("sorts by label, then dir, regardless of input order", () => {
    const ds = [at(1, "/z/beta/f"), at(2, "/a/alpha/f"), at(3, "/q/beta/f")];
    const dirs = (input: typeof ds) => deriveFolders(input).map((f) => f.dir);
    expect(dirs(ds)).toEqual(["/a/alpha", "/q/beta", "/z/beta"]);
    expect(dirs([...ds].reverse())).toEqual(["/a/alpha", "/q/beta", "/z/beta"]);
  });

  it("is empty for no downloads", () => {
    expect(deriveFolders([])).toEqual([]);
  });
});
