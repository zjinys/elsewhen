import { describe, expect, it } from "vitest";
import {
  assignFields,
  dotPath,
  jsonSource,
  parseCwsDetail,
  parseCwsSidebar,
} from "../src/sources";

describe("dotPath", () => {
  it("resolves nested paths", () => {
    expect(dotPath({ a: { b: [1, 2] } }, "a.b.1")).toBe(2);
  });
  it("returns undefined on miss", () => {
    expect(dotPath({ a: 1 }, "x.y")).toBeUndefined();
  });
  it("feed.results path for apple rss", () => {
    const data = { feed: { results: [{ id: 1 }, { id: 2 }] } };
    expect(dotPath(data, "feed.results")).toHaveLength(2);
  });
});

describe("assignFields", () => {
  it("flattens dot paths into readable keys", () => {
    const item: Record<string, unknown> = {};
    assignFields(item, { license: { spdx_id: "MIT" }, ratings: { average: 4.9 } }, [
      "license.spdx_id",
      "ratings.average",
    ]);
    // 扁平后仍保留语义（不是裸 spdx_id）。
    expect(item).toEqual({ license_spdx_id: "MIT", ratings_average: 4.9 });
  });

  it("drops null/undefined instead of writing them", () => {
    const item: Record<string, unknown> = {};
    assignFields(item, { a: null, b: 1 }, ["a", "b", "missing"]);
    expect(item).toEqual({ b: 1 });
  });

  it("array index paths work", () => {
    const item: Record<string, unknown> = {};
    assignFields(item, { genres: [{ name: "News", url: "x" }] }, ["genres.0.name"]);
    expect(item).toEqual({ genres_0_name: "News" });
  });
});

describe("specHash / shape", () => {
  it("字段清单变化会改变 specHash", () => {
    const a = jsonSource("s", {
      url: "https://x", itemsPath: "items", itemId: "id",
      fields: ["a"], intervalMinutes: 10,
    });
    const b = jsonSource("s", {
      url: "https://x", itemsPath: "items", itemId: "id",
      fields: ["a", "b"], intervalMinutes: 10,
    });
    expect(a.specHash).not.toEqual(b.specHash);
  });

  it("相同字段清单的 specHash 稳定", () => {
    const mk = () =>
      jsonSource("s", {
        url: "https://x", itemsPath: "items", itemId: "id",
        fields: ["a", "b"], intervalMinutes: 10,
      });
    expect(mk().specHash).toEqual(mk().specHash);
  });

  it("withRank 参与指纹", () => {
    const base = { url: "https://x", itemsPath: "i", itemId: "id" as const,
      fields: ["a"], intervalMinutes: 10 };
    expect(jsonSource("s", base).specHash).not.toEqual(
      jsonSource("s", { ...base, withRank: true }).specHash
    );
  });
});

describe("parseCwsSidebar：开发者行的两种形态", () => {
  it("Offered by：单行值", () => {
    const html = `<li><div class="QDHp8e">Offered by</div><div>Raymond Hill (gorhill)</div></li>
      <li><div class="QDHp8e">Size</div><div>8.66MiB</div></li>`;
    const rows = parseCwsSidebar(html);
    expect(rows["offered by"]).toBe("Raymond Hill (gorhill)");
    expect(rows["size"]).toBe("8.66MiB");
  });

  it("Developer：值嵌在多层 div 里且是多行地址，只取第一行", () => {
    // 真实 MetaMask 页：值不是纯文本，旧正则要求「无标签」会取成空串
    const html = `<li class="MqICNe"><div class="QDHp8e">Developer</div><div><div class="yyjN4"><div class="mdSapd">Consensys Software Inc.<br>5049 Edwards Ranch Rd
Fort Worth, TX 76109-4213<br>US</div><a href="https://metamask.io/">site</a></div></div></li>
      <li><div class="QDHp8e">Updated</div><div>November 16, 2026</div></li>`;
    const rows = parseCwsSidebar(html);
    expect(rows["developer"]).toBe("Consensys Software Inc.");
    expect(rows["updated"]).toBe("November 16, 2026");
  });

  it("不把下一行的 Size/Languages 吞进当前值", () => {
    const html = `<li><div class="QDHp8e">Updated</div><div>November 16, 2026</div></li>
      <li><div class="QDHp8e">Size</div><div>24.04MiB</div></li>`;
    expect(parseCwsSidebar(html)["updated"]).toBe("November 16, 2026");
  });
});

describe("parseCwsDetail：页面归属判定", () => {
  // 真实页面里 rating 数据块的形状（简化但结构一致）
  // 侧栏行在真实页面里是 <li>，值区间靠 </li> 界定（见 parseCwsSidebar）
  const detail = (name: string, rating: string, count: number, extra = "") =>
    `"https://lh3.googleusercontent.com/abc","${name}",${rating},${count},"https://lh3.googleusercontent.com/def"` +
    `<li><div class="QDHp8e">Offered by</div><div>${extra}</div></li>`;

  const METAMASK = "nkbihfbeogaeaoehlefnkodbefgpgknn";

  it("正常页：canonical 指向自身，取到本扩展的评分", () => {
    const html = `<title>MetaMask - Chrome Web Store</title>
<link rel="canonical" href="https://chromewebstore.google.com/detail/metamask/${METAMASK}?hl=en">
${detail("MetaMask", "2.0656565656565657", 5938, "ConsenSys")}
<div class="QDHp8e">Updated</div><div>November 16, 2026</div>
,1,null,12000000,1,`;
    const item = parseCwsDetail(
      html,
      `https://chromewebstore.google.com/detail/metamask/${METAMASK}?hl=en`,
      METAMASK
    );
    expect(item.error).toBeUndefined();
    expect(item.name).toBe("MetaMask");
    expect(item.rating).toBe(2.07);
    expect(item.rating_count).toBe(5938);
    expect(item.developer).toBe("ConsenSys");
    expect(item.users).toBe(12000000);
  });

  it("已下架：跳首页时报 not available，不把首页里别人的数据当成自己的", () => {
    const html = `<title>Chrome Web Store</title>
<link rel="canonical" href="https://chromewebstore.google.com/">
${detail("Dark Reader", "4.290977443609022", 13293)}
${detail("Volume Master", "4.839149618933012", 49851)}`;
    const item = parseCwsDetail(
      html,
      "https://chromewebstore.google.com/?hl=en",
      "cjpalhdlnbpafiamejdnhjbkeiagm"
    );
    expect(item.error).toContain("not available");
    expect(item.rating).toBeUndefined();
  });

  // 线上实测（wrangler tail）：uBlock Origin 的页面停在
  // /detail/empty-title/<id>，**没有 canonical**，title 只是 "Chrome Web Store"，
  // 但页面仍有 ~710KB 推荐卡片，含邻居 (uBlock Origin Lite) 的 name/rating。
  // 旧逻辑的泛匹配兜底正是在这里抓到 4.5/3831，让 uBlock Origin 长期显示
  // 别人的数据；又因数据恒定不变，永远不产生 finding——静默的假条目。
  it("empty-title 页（无 canonical）不猜数据", () => {
    const html = `<title>Chrome Web Store</title>
${detail("uBlock Origin Lite", "4.5", 3831)}
${detail("MetaMask", "2.07", 5938)}`;
    const item = parseCwsDetail(
      html,
      "https://chromewebstore.google.com/detail/empty-title/cjpalhdlnbpafiamejdnhjbkeiagm?hl=en",
      "cjpalhdlnbpafiamejdnhjbkeiagm"
    );
    expect(item.error).toContain("not available");
    expect(item.name).toBeUndefined();
    expect(item.rating).toBeUndefined();
    expect(item.rating_count).toBeUndefined();
  });

  it("canonical 指向别的扩展时报 mismatch", () => {
    const html = `<title>MetaMask - Chrome Web Store</title>
<link rel="canonical" href="https://chromewebstore.google.com/detail/metamask/${METAMASK}?hl=en">
${detail("MetaMask", "2.07", 5938)}`;
    const item = parseCwsDetail(
      html,
      "https://chromewebstore.google.com/detail/x/ddkjiahejlhfcafbddmgiahcphecmpfh?hl=en",
      "ddkjiahejlhfcafbddmgiahcphecmpfh"
    );
    expect(item.error).toContain("mismatch");
  });

  it("页面里有邻居数据时，名字锚点取到本扩展而非靠前的邻居", () => {
    const html = `<title>uBlock Origin Lite - Chrome Web Store</title>
<link rel="canonical" href="https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh?hl=en">
${detail("Phantom", "2.7299881234", 1367)}
${detail("Keplr", "4.7975", 10762)}
${detail("uBlock Origin Lite", "4.5", 3831, "Raymond Hill (gorhill)")}
,1,1,22000000,null,`;
    const item = parseCwsDetail(
      html,
      "https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh?hl=en",
      "ddkjiahejlhfcafbddmgiahcphecmpfh"
    );
    expect(item.name).toBe("uBlock Origin Lite");
    expect(item.rating).toBe(4.5);
    expect(item.rating_count).toBe(3831);
    expect(item.users).toBe(22000000);
    expect(item.developer).toBe("Raymond Hill (gorhill)");
  });
});
