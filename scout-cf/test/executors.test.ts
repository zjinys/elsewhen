import { afterEach, describe, expect, it, vi } from "vitest";
import { cwsSource, inertiaSource } from "../src/sources";

afterEach(() => {
  vi.unstubAllGlobals();
});

/** 最小 D1 stub：只支持 getSetting 需要的 first() 调用 */
const d1WithSettings = (value: string | null) =>
  ({
    prepare: () => ({
      bind: () => ({
        first: async () => (value === null ? null : { value }),
      }),
    }),
  }) as never;


// ---------- inertia ----------

function inertiaHtml(pageJson: object): string {
  const escaped = JSON.stringify(pageJson)
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#x27;");
  return `<html><body><div data-page="${escaped}"></div></body></html>`;
}

describe("inertiaSource", () => {
  it("extracts products from data-page JSON", async () => {
    const html = inertiaHtml({
      props: {
        search_results: {
          products: [
            { permalink: "abc", name: "Kit A", price_cents: 1999, ratings: { count: 3, average: 4.5 } },
            { permalink: "def", name: "Kit B", price_cents: 0 },
          ],
        },
      },
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(html, { status: 200 }))
    );
    const spec = inertiaSource("t", {
      url: "https://gumroad.com/discover",
      params: { query: "x" },
      itemsPath: "props.search_results.products",
      itemId: "permalink",
      fields: ["permalink", "name", "price_cents", "ratings"],
      intervalMinutes: 60,
    });
    const items = await spec.fetch({ DB: d1WithSettings(null) } as never, { http: fetch, env: {} as never });
    expect(items).toHaveLength(2);
    expect(items[0].id).toBe("abc");
    expect(items[0].price_cents).toBe(1999);
    expect(items[1].name).toBe("Kit B");
  });

  it("throws a clear error when data-page is missing (bot wall)", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("<html>Just a moment...</html>", { status: 200 }))
    );
    const spec = inertiaSource("t", {
      url: "https://x.test/",
      itemsPath: "props.p",
      itemId: "id",
      intervalMinutes: 60,
    });
    await expect(spec.fetch({ DB: d1WithSettings(null) } as never, { http: fetch, env: {} as never })).rejects.toThrow(
      /no data-page/
    );
  });

  it("throws when items_path is absent", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(inertiaHtml({ props: {} }), { status: 200 }))
    );
    const spec = inertiaSource("t", {
      url: "https://x.test/",
      itemsPath: "props.missing",
      itemId: "id",
      intervalMinutes: 60,
    });
    await expect(spec.fetch({ DB: d1WithSettings(null) } as never, { http: fetch, env: {} as never })).rejects.toThrow(
      /items_path/
    );
  });
});

// ---------- cws ----------

/** 详情页 fixture 带 title + canonical——解析器靠它判定「这页是不是这个扩展」。
 *  extId 默认 e*32，与测试里用的 id 对齐。 */
const cwsPage = (
  name: string,
  rating: string,
  count: string,
  userSegment: string,
  extId: string = "e".repeat(32)
): string =>
  `<html><head><title>${name} - Chrome Web Store</title><link rel="canonical" href="https://chromewebstore.google.com/detail/slug/${extId}?hl=en"></head><script>AF_initDataCallback({key: 'ds:1', data:["x",
    "https://lh3.googleusercontent.com/aaa","${name}",${rating},${count},"https://lh3.googleusercontent.com/bbb",
    "desc",null,null,1,null,["x",null,22],${userSegment},null,"https://lh3.googleusercontent.com/c",[1,2]]});</script></html>`;

describe("cwsSource", () => {
  it("parses the uBlock user-count shape", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(cwsPage("uBlock Origin Lite", "4.5", "3831", "1,1,22000000"), { status: 200 })
      )
    );
    const spec = cwsSource("cws", { extensionIds: ["e".repeat(32)], intervalMinutes: 60 });
    const items = await spec.fetch({ DB: d1WithSettings(null) } as never, { http: fetch, env: {} as never });
    expect(items[0]).toMatchObject({
      id: "e".repeat(32),
      name: "uBlock Origin Lite",
      rating: 4.5,
      rating_count: 3831,
      users: 22000000,
    });
  });

  it("parses the MetaMask user-count shape and clamps rating precision", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(cwsPage("MetaMask", "2.0656565656565657", "5938", "1,null,12000000,1"), { status: 200 })
      )
    );
    const spec = cwsSource("cws", { extensionIds: ["e".repeat(32)], intervalMinutes: 60 });
    const items = await spec.fetch({ DB: d1WithSettings(null) } as never, { http: fetch, env: {} as never });
    expect(items[0]).toMatchObject({ name: "MetaMask", rating: 2.07, users: 12000000 });
  });

  it("follows one same-origin slug redirect", async () => {
    const mock = vi.fn(async (url: string | URL | Request) => {
      const u = typeof url === "string" ? url : url instanceof Request ? url.url : url.toString();
      if (u.includes("detail/x/")) {
        return new Response(null, {
          status: 302,
          headers: { location: "https://chromewebstore.google.com/detail/real-slug/e".repeat(1).slice(0, 0) + `https://chromewebstore.google.com/detail/real-slug/${"e".repeat(32)}?hl=en` },
        });
      }
      return new Response(cwsPage("Ext", "4.0", "12", "1,1,50000"), { status: 200 });
    });
    vi.stubGlobal("fetch", mock);
    const spec = cwsSource("cws", { extensionIds: ["e".repeat(32)], intervalMinutes: 60 });
    const items = await spec.fetch({ DB: d1WithSettings(null) } as never, { http: fetch, env: {} as never });
    expect(items[0].name).toBe("Ext");
    expect(mock).toHaveBeenCalledTimes(2);
  });

  it("records per-item error instead of failing the batch", async () => {
    const mock = vi.fn(async (url: string | URL | Request) => {
      const u = typeof url === "string" ? url : url instanceof Request ? url.url : url.toString();
      if (u.includes("bad")) return new Response(null, { status: 404 });
      return new Response(cwsPage("Good", "4.0", "12", "1,1,50000", "g".repeat(32)), { status: 200 });
    });
    vi.stubGlobal("fetch", mock);
    const spec = cwsSource("cws", {
      extensionIds: ["bad0000000000000000000000000000", "g".repeat(32)],
      intervalMinutes: 60,
    });
    const items = await spec.fetch({ DB: d1WithSettings(null) } as never, { http: fetch, env: {} as never });
    expect(items[0]).toEqual({ id: "bad0000000000000000000000000000", error: "HTTP 404" });
    expect(items[1].name).toBe("Good");
  });
});
