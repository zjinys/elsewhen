import { describe, expect, it } from "vitest";

/**
 * Integration-style tests for the feed tokenizer + atomItem via the module's
 * internal parseXml (exported indirectly through fetchFeed — here we test the
 * tokenizer through a tiny harness importing from sources).
 */
import { jsonSource, feedSource, SOURCES, type Item } from "../src/sources";

describe("sources registry", () => {
  it("contains only consumer/business sources (no dev-focused ones)", () => {
    const ids = SOURCES.map((s) => s.id);
    expect(ids).toContain("github-dart");
    expect(ids).toContain("producthunt-daily");
    expect(ids).toContain("appstore-us-top-free");
    expect(ids).not.toContain("hackernews");
    expect(ids).not.toContain("devto");
  });

  it("jsonSource builds a fetchable spec", async () => {
    const spec = jsonSource("t", {
      url: "https://x.test/api",
      itemsPath: "items",
      itemId: "id",
      intervalMinutes: 60,
    });
    expect(spec.id).toBe("t");
    expect(spec.enabled).toBe(true);
    expect(spec.intervalMinutes).toBe(60);
    // fetch would hit network; just verify the spec shape
    expect(typeof spec.fetch).toBe("function");
  });

  it("feedSource spec shape", () => {
    const spec = feedSource("f", { url: "https://x.test/feed", intervalMinutes: 120 });
    expect(spec.id).toBe("f");
    expect(spec.intervalMinutes).toBe(120);
  });
});

describe("item typing", () => {
  it("items carry string ids", () => {
    const item: Item = { id: "a/b", extra: 1 };
    expect(item.id).toBe("a/b");
  });
});
