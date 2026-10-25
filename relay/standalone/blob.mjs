/**
 * relay v3 数据面：大对象（快照字节流）走这六个端点，不再当「信」塞进信箱。
 *
 * 为什么必须另开一层（事故根因）：
 *   - 信件表用自增 id 当身份 ⇒ 每次重发都是一行新信，同一份快照在 relay 上留 4.4 份；
 *   - 接收方按 `id>after` 翻页 ⇒ 断点只能整批重来；
 *   - 整份 122 MB 全等在 relay 上，等接收方「整份 ack」才删 ⇒ 峰值 = 配额上限。
 * 这里把身份改成 **(blob_id, idx)**：重复 PUT 同一片物理上不可能占第二份空间，
 * `ack upto` 立即释放，`close` 精确清零，峰值 = 客户端窗口。
 *
 * 盲性不变：relay 只见 opaque 字节 + 长度 + 索引，`digest` 是客户端算的哈希（也是
 * opaque），不解析任何内容。
 *
 * 鉴权全部复用 `server.mjs` 的 `stamp()` / 签名域 / sender credential：
 *   POST /blob/open   {blob_id}                             域 blob-open  **sender 签名**
 *   PUT  /blob/chunk  ?blob_id&index   body=原始密文字节     域 blob-put   **sender 签名**
 *   GET  /blob/state  ?blob_id                              域 blob-state **owner 或开户 sender**
 *   GET  /blob/chunk  ?blob_id&from&limit                   域 blob-get   **owner 签名**
 *   POST /blob/ack    {blob_id, upto}                       域 blob-ack   **owner 签名**
 *   POST /blob/close  {blob_id, status}                     域 blob-close **owner 或 sender**
 *
 * 方案原文写「open 由 owner 签名」，实现取 **sender 签名**：上传方在 v2 里本来就是
 * 接收方信箱里的 sender 身份（`/register` 授权的那个），要求它持 owner 凭据等于把
 * 接收方主密钥（能读/能删整个信箱）交给上传方，这与盲中继的信任模型冲突。读取与
 * 确认侧（state/get/ack）才是 owner —— 数据是往他家里放的，只有他能拆。
 *
 * 配额分账：blob 有自己的 `BLOB_QUOTA_BYTES`（默认 64 MB，在途窗口之和），与信件的
 * `MAILBOX_QUOTA_BYTES` **互不占用** ⇒ 「快照把控制消息挤死」在结构上不可表达。
 */
/** `blob_id` 由客户端生成（随机/哈希），只允许 URL 安全字符且不含任何分隔符。 */
const BLOB_ID = /^[A-Za-z0-9_-]{16,64}$/;
/** 单个 blob 的分片数上限（快照 174 片，留足余量；也挡住「无限多小片」的滥用）。
 *
 * 这是 relay 侧的**协议上限**。客户端的校验上界更严（`WINDOW_CHUNKS*256 = 2560`，
 * 见 `src/sync/transport_relay/blob.rs`）—— 两边故意不是同一个数：relay 管「别人能
 * 占多少」，客户端管「自己最多接受多大的片号」。改其中一个不必同步改另一个，
 * 但两边都不得小于真实快照片数。 */
export const MAX_BLOB_CHUNKS = 4096;
/** 单次 PUT 的字节上限（客户端分片 256 KiB，4× 余量给将来的更大分片）。 */
export const CHUNK_BYTES_MAX = 1024 * 1024;
/** 单次 GET 最多返回多少片。
 *
 * 12 × `CHUNK_BYTES_MAX` = 12 MiB，必须留在 `RESPONSE_BYTES`（16 MB）以内 ——
 * 否则「合法的窗口请求」会撞上响应上限，接收端拿不到完整一窗，窗口流控就死锁。
 * 实测一片 256 KiB 明文封成信封后约 683 KiB（多层 base64），所以按字节上限
 * 而不是按片数凭感觉定窗口，是唯一稳妥的做法。 */
export const GET_CHUNKS_MAX = 12;
/** 无进展 TTL：30 分钟没有任何 PUT/ack 就整块回收（不是信件的 7 天）。 */
export const BLOB_TTL_MS_DEFAULT = 30 * 60_000;
/** 在途 blob 的字节上限（= 方案里的 BLOB_WINDOW_MAX）。 */
export const BLOB_QUOTA_BYTES_DEFAULT = 64 * 1024 * 1024;
/** 同一信箱同时在途的 blob 数上限（一次只该有一个快照在传，留 32 给并发配对）。 */
export const MAX_OPEN_BLOBS = 32;

class Blobs {
  constructor(mailbox, ctx, limits) {
    this.mailbox = mailbox;
    this.ctx = ctx;
    this.limits = limits;
  }
  get db() { return this.mailbox.db; }

  present(blobId, from = 0, limit = MAX_BLOB_CHUNKS) {
    return this.ctx.sqlAll(this.db, 'SELECT idx, LENGTH(ciphertext) AS size FROM relay_v3_blob_chunk WHERE blob_id=? AND idx>=? ORDER BY idx LIMIT ?', blobId, from, limit)
      .map((row) => ({ index: Number(row.idx), size: Number(row.size) }));
  }
  blob(blobId) {
    return this.ctx.sqlGet(this.db, 'SELECT * FROM relay_v3_blob WHERE blob_id=?', blobId);
  }
  /** 该信箱所有在途 blob 的**实际**落盘字节（配额与 /status 都看这个）。 */
  storedBytes() {
    return Number(this.ctx.sqlGet(this.db, 'SELECT COALESCE(SUM(LENGTH(ciphertext)),0) AS bytes FROM relay_v3_blob_chunk').bytes);
  }
  /** 收件方（owner）。 */
  ownerOperation(body, domain, parts, apply) {
    return this.mailbox.ownerOperation(body, domain, parts, apply);
  }
  /** 上传方（sender）：与 `/send` 同一套授权检查，只是签名域不同。 */
  senderOperation(body, domain, parts, digest, apply) {
    const { require, hex, signed, verifyHmac, withTx } = this.ctx;
    const auth = this.ctx.stamp(body);
    const sender = body.sender_mb;
    require(hex(sender, 64), 400, 'invalid sender_mb');
    this.mailbox.owner(body.mailbox_key);
    const credential = this.mailbox.sender(sender);
    require(credential?.cred_hex && credential.expires_at > Date.now(), 403, 'sender not authorized');
    const message = signed(domain, [sender, body.mailbox_key, ...parts], auth);
    if (digest !== undefined) message.push(digest);
    require(verifyHmac(credential.cred_hex, message, auth.sig), 403, 'bad sender signature');
    return withTx(this.db, () => {
      const now = Date.now(), current = this.mailbox.sender(sender);
      require(current?.cred_hex && current.revision === credential.revision && current.expires_at > now, 403, 'sender authorization changed');
      this.mailbox.clean(now);
      this.mailbox.claim(auth, now);
      return apply(now);
    });
  }

  open(body) {
    const { require } = this.ctx;
    require(BLOB_ID.test(body.blob_id ?? ''), 400, 'invalid blob id');
    const blobId = body.blob_id;
    return this.senderOperation(body, 'blob-open', [blobId], undefined, (now) => {
      const existing = this.blob(blobId);
      if (existing) {
        // 幂等：丢响应后重试只续期并回报已有的分片，绝不重置进度。
        // 片数/总字节/摘要在流式上传里发完才知道，所以这里没有「元数据不符」分支。
        require(existing.sender_mb === body.sender_mb, 403, 'blob belongs to another sender');
        this.ctx.sqlRun(this.db, 'UPDATE relay_v3_blob SET expires_at=? WHERE blob_id=?', now + this.limits.ttl, blobId);
        // `created:false` 让上传方分清两种"空"：一片都没传过 vs 全部被 ack 光了。
        // 没有这个信号，续传会把「relay 上的 blob 早被 TTL 收掉」误判成「都已送达」。
        return { ok: true, version: this.ctx.version, blob_id: blobId, created: false, have: this.present(blobId).map((c) => c.index) };
      }
      const count = Number(this.ctx.sqlGet(this.db, 'SELECT COUNT(*) AS n FROM relay_v3_blob').n);
      require(count < MAX_OPEN_BLOBS, 507, 'too many open blobs');
      // 记下开这个 blob 的 sender：写入与撤回都只认它，别让同信箱里另一个
      // sender 用合法的 sender 凭据去 close/覆盖不属于它的传输。
      this.ctx.sqlRun(this.db, 'INSERT INTO relay_v3_blob (blob_id,sender_mb,created_at,expires_at) VALUES (?,?,?,?)',
        blobId, body.sender_mb, now, now + this.limits.ttl);
      return { ok: true, version: this.ctx.version, blob_id: blobId, created: true, have: [] };
    });
  }

  put(query, payload) {
    const { require, integer, sha256Hex } = this.ctx;
    require(BLOB_ID.test(query.blob_id ?? ''), 400, 'invalid blob id');
    const index = Number(query.index);
    require(integer(index, 0, MAX_BLOB_CHUNKS - 1), 400, 'invalid chunk index');
    require(payload.length > 0 && payload.length <= CHUNK_BYTES_MAX, 413, 'chunk too large');
    const blobId = query.blob_id;
    return this.senderOperation(query, 'blob-put', [blobId, String(index)], sha256Hex(payload), (now) => {
      const blob = this.blob(blobId);
      require(blob, 404, 'blob not open');
      require(blob.sender_mb === query.sender_mb, 403, 'blob belongs to another sender');
      // 配额按**实际落盘字节**逐片判：重投同一片要扣掉它自己原有的长度，
      // 否则同尺寸覆盖会被自己挤成 507。总量不知道也照样能守住上限。
      const stored = this.storedBytes();
      const previous = Number(this.ctx.sqlGet(this.db, 'SELECT LENGTH(ciphertext) AS n FROM relay_v3_blob_chunk WHERE blob_id=? AND idx=?', blobId, index)?.n ?? 0);
      require(stored - previous + payload.length <= this.limits.quota, 507, 'blob quota exceeded');
      // 主键 (blob_id, idx) 让重投天然幂等：写入同一片只是原地覆盖，不产生第二份。
      this.ctx.sqlRun(this.db, 'INSERT INTO relay_v3_blob_chunk (blob_id,idx,ciphertext) VALUES (?,?,?) ON CONFLICT(blob_id,idx) DO UPDATE SET ciphertext=excluded.ciphertext',
        blobId, index, payload);
      this.ctx.sqlRun(this.db, 'UPDATE relay_v3_blob SET expires_at=? WHERE blob_id=?', now + this.limits.ttl, blobId);
      const present = this.present(blobId);
      return { ok: true, index, have: present.length, highest: present.at(-1)?.index ?? -1 };
    });
  }

  /**
   * 进度查询。owner 看自己信箱里的传输，**开户 sender 也允许**：上传方要维持
   * 窗口就必须知道接收方 ack 到哪了，否则只能靠撞 507 退避。两者都验签名，
   * sender 还必须确实是这个 blob 的开户人；不暴露任何内容，只有分片索引。
   *
   * 不返回片数/总字节/摘要：流式上传时 relay 不知道这些，总量以端到端签名的
   * manifest 为准。
   */
  state(query) {
    const { require, verifyHmac, signed } = this.ctx;
    require(BLOB_ID.test(query.blob_id ?? ''), 400, 'invalid blob id');
    const blobId = query.blob_id;
    const auth = this.ctx.stamp(query);
    const owner = this.mailbox.owner(query.mailbox_key);
    const sender = typeof query.sender_mb === 'string' ? query.sender_mb : null;
    const credential = sender && this.ctx.hex(sender, 64) ? this.mailbox.sender(sender) : null;
    const parts = signed('blob-state', [query.mailbox_key, blobId], auth);
    const byOwner = verifyHmac(owner.cred_hex, parts, auth.sig);
    const opened = this.blob(blobId);
    const bySender = !!credential?.cred_hex && credential.expires_at > Date.now()
      && opened?.sender_mb === sender && verifyHmac(credential.cred_hex, parts, auth.sig);
    require(byOwner || bySender, 403, 'bad blob state signature');
    // 签名已经验过（owner 或开户 sender 之一），这里只需要 nonce 防重放 + 过期回收，
    // 不能再走 ownerOperation（它会拿 owner 凭据再验一遍，sender 调用必挂）。
    const { withTx } = this.ctx;
    return withTx(this.db, () => {
      const now = Date.now();
      this.mailbox.clean(now);
      this.mailbox.claim(auth, now);
      const blob = this.blob(blobId);
      const present = blob ? this.present(blobId) : [];
      return {
        ok: true, version: this.ctx.version, blob_id: blobId,
        expires_at: blob ? Number(blob.expires_at) : 0,
        highest: present.at(-1)?.index ?? -1,
        stored_bytes: present.reduce((sum, chunk) => sum + chunk.size, 0),
        present: present.map((chunk) => chunk.index),
      };
    });
  }

  /**
   * 裸二进制下载：`[4 字节大端长度][密文]` 连续帧，不套 JSON。
   * 省掉 base64 的 1.37× 膨胀，也让客户端能按窗口边收边写盘。
   */
  fetch(query) {
    const { require, integer } = this.ctx;
    require(BLOB_ID.test(query.blob_id ?? ''), 400, 'invalid blob id');
    const from = query.from === undefined ? 0 : Number(query.from);
    const limit = query.limit === undefined ? 8 : Number(query.limit);
    require(integer(from, 0, MAX_BLOB_CHUNKS - 1), 400, 'invalid chunk cursor');
    require(integer(limit, 1, GET_CHUNKS_MAX), 400, 'invalid chunk limit');
    const blobId = query.blob_id;
    return this.ownerOperation(query, 'blob-get', [query.mailbox_key, blobId, String(from), String(limit)], () => {
      const blob = this.blob(blobId);
      require(blob, 404, 'blob not open');
      const rows = this.ctx.sqlAll(this.db, 'SELECT idx, ciphertext FROM relay_v3_blob_chunk WHERE blob_id=? AND idx>=? ORDER BY idx LIMIT ?', blobId, from, limit);
      // 只返回**从第一片起连续**的那一段：中间有缺口就让客户端先去补缺口（`state` 会给 missing），
      // 而不是把后面不相邻的片段也塞进同一个流里让它错位拼接。
      const frames = [];
      let expected = rows.length ? Number(rows[0].idx) : -1;
      for (const row of rows) {
        if (Number(row.idx) !== expected) break;
        frames.push(row.ciphertext);
        expected += 1;
      }
      const head = Buffer.alloc(4 * frames.length);
      frames.forEach((frame, i) => head.writeUInt32BE(frame.length, i * 4));
      const parts = [];
      frames.forEach((frame, i) => parts.push(head.subarray(i * 4, i * 4 + 4), frame));
      return {
        binary: Buffer.concat(parts),
        from: frames.length ? Number(rows[0].idx) : -1,
        count: frames.length,
        next: frames.length ? expected : -1,
      };
    });
  }

  /** 逐片释放：收件方确认「index ≤ upto 已落盘」后立刻删掉这些分片。 */
  ack(body) {
    const { require, integer } = this.ctx;
    require(BLOB_ID.test(body.blob_id ?? ''), 400, 'invalid blob id');
    require(integer(body.upto, 0, MAX_BLOB_CHUNKS - 1), 400, 'invalid ack cursor');
    const blobId = body.blob_id, upto = body.upto;
    return this.ownerOperation(body, 'blob-ack', [body.mailbox_key, blobId, String(upto)], (now) => {
      const blob = this.blob(blobId);
      require(blob, 404, 'blob not open');
      const deleted = this.ctx.sqlRun(this.db, 'DELETE FROM relay_v3_blob_chunk WHERE blob_id=? AND idx<=?', blobId, upto).changes;
      this.ctx.sqlRun(this.db, 'UPDATE relay_v3_blob SET expires_at=? WHERE blob_id=?', now + this.limits.ttl, blobId);
      const present = this.present(blobId);
      return { ok: true, deleted: Number(deleted), remaining: present.length, highest: present.at(-1)?.index ?? -1 };
    });
  }

  /** 结束：精确回收整个 blob（不靠 TTL）。owner 结束，sender 也可撤回。 */
  close(body) {
    const { require, verifyHmac, signed, withTx } = this.ctx;
    require(BLOB_ID.test(body.blob_id ?? ''), 400, 'invalid blob id');
    require(body.status === 'done' || body.status === 'cancel', 400, 'invalid close status');
    const blobId = body.blob_id, status = body.status;
    const sender = typeof body.sender_mb === 'string' ? body.sender_mb : null;
    // 两个可能的签名者：owner（收件方说结束）与 sender（上传方撤回）。
    // 两把凭据都试一遍且不短路，避免「哪个凭据错了」这种可探测的时序/分支差异。
    const auth = this.ctx.stamp(body);
    const owner = this.mailbox.owner(body.mailbox_key);
    const credential = sender && this.ctx.hex(sender, 64) ? this.mailbox.sender(sender) : null;
    const parts = signed('blob-close', [body.mailbox_key, blobId, status], auth);
    const byOwner = verifyHmac(owner.cred_hex, parts, auth.sig);
    // sender 只能撤回**自己开的**那个 blob（表里记了 sender_mb）；别人的传输得由 owner 关。
    const opened = this.blob(blobId);
    const bySender = !!credential?.cred_hex && credential.expires_at > Date.now()
      && opened?.sender_mb === sender && verifyHmac(credential.cred_hex, parts, auth.sig);
    require(byOwner || bySender, 403, 'bad close signature');
    return withTx(this.db, () => {
      const now = Date.now();
      this.mailbox.clean(now);
      this.mailbox.claim(auth, now);
      const blob = this.blob(blobId);
      // 幂等：已经关掉（或超时被收掉）的 blob 重复 close 也是成功，别再报错让客户端重试。
      if (!blob) return { ok: true, status, freed: 0 };
      const freed = this.present(blobId).reduce((sum, chunk) => sum + chunk.size, 0);
      this.ctx.sqlRun(this.db, 'DELETE FROM relay_v3_blob_chunk WHERE blob_id=?', blobId);
      this.ctx.sqlRun(this.db, 'DELETE FROM relay_v3_blob WHERE blob_id=?', blobId);
      return { ok: true, status, freed };
    });
  }
}

/**
 * `/blob/*` 的 HTTP 入口。整段交给本模块，`server.mjs` 只在 switch 之前分一次流，
 * 因为 `PUT /blob/chunk` 的 body 是**裸二进制**，不能走 JSON 解析那条路。
 */
export async function handleBlobRequest(req, res, url, ctx) {
  const { require, json, failure, send, queryBody, boundedBinary, openMailbox, Mailbox } = ctx;
  try {
    const path = url.pathname, method = req.method;
    const routes = {
      '/blob/open': 'POST', '/blob/chunk': method === 'PUT' ? 'PUT' : 'GET',
      '/blob/state': 'GET', '/blob/ack': 'POST', '/blob/close': 'POST',
    };
    require(routes[path] !== undefined, 404, 'not found');
    require(method === routes[path], 405, 'method not allowed');
    let query;
    if (method === 'PUT') {
      query = queryBody(url);
      require(query.blob_id !== undefined && query.index !== undefined, 400, 'blob id and chunk index required');
      // 版本/信箱/「不能拿 sender 记录当收件凭据」三条契约与其余端点是同一份
      // （`server.mjs` 的 `validateEnvelope`）。原来这里手抄了其中两条，
      // 于是 `PUT` 少了一条 `recipient_mb` 约束 —— 契约一旦要改就会漏掉这条路径。
      ctx.validateEnvelope(query);
      const payload = await boundedBinary(req, CHUNK_BYTES_MAX);
      const mailbox = new Mailbox(openMailbox(query.mailbox_key));
      const blobs = new Blobs(mailbox, ctx, ctx.blobLimits);
      const result = blobs.put(query, payload);
      return send(res, json(result));
    }
    const body = method === 'POST' ? ctx.parsedJson(await ctx.boundedBody(req, 32 * 1024)) : queryBody(url);
    ctx.validateEnvelope(body);
    const mailbox = new Mailbox(openMailbox(body.mailbox_key));
    const blobs = new Blobs(mailbox, ctx, ctx.blobLimits);
    if (path === '/blob/open') return send(res, json(blobs.open(body)));
    if (path === '/blob/ack') return send(res, json(blobs.ack(body)));
    if (path === '/blob/close') return send(res, json(blobs.close(body)));
    if (path === '/blob/state') return send(res, json(blobs.state(body)));
    const page = blobs.fetch(body);
    return send(res, {
      status: 200,
      headers: {
        'content-type': 'application/octet-stream',
        'cache-control': 'no-store',
        'x-blob-from': String(page.from),
        'x-blob-count': String(page.count),
        'x-blob-next': String(page.next),
      },
      body: page.binary,
    });
  } catch (error) {
    return send(res, failure(error));
  }
}
