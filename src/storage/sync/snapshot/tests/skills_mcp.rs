use super::export;
use super::hex_of;
use super::pair;
use super::stage;
use super::*;

#[test]
fn skill_distribution_travels_inside_snapshot() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    // A 端：技能发布两文件 + 一条墓碑（模拟曾卸载过的旧技能）。
    a.store
        .connection
        .execute(
            "INSERT INTO skill_files(skill_id,rel,bytes,sha256,size,updated_at)
             VALUES('proxy-pick','SKILL.md',?1,?2,3,'2026-10-18T08:00:00Z'),
                   ('proxy-pick','scripts/pick.py',x'70697961',?3,4,'2026-10-18T08:00:00Z')",
            rusqlite::params![
                b"abc".as_slice(),
                {
                    use ring::digest::{SHA256, digest};
                    let d = digest(&SHA256, b"abc");
                    hex_of(d.as_ref())
                },
                {
                    use ring::digest::{SHA256, digest};
                    let d = digest(&SHA256, b"piya");
                    hex_of(d.as_ref())
                },
            ],
        )
        .unwrap();
    a.store
        .connection
        .execute(
            "INSERT INTO skill_tombstones(skill_id,updated_at) VALUES('old-skill','2026-10-17T00:00:00Z')",
            [],
        )
        .unwrap();

    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    b.store
        .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
        .unwrap();

    // 技能文件与墓碑随快照到达（分发的「随整库走」在此断言，不许再静默断档）。
    assert_eq!(b.count("skill_files"), 2, "两枚技能文件应随快照到达 B");
    assert_eq!(b.count("skill_tombstones"), 1, "墓碑应随快照到达 B");
    let bytes: Vec<u8> = b
        .store
        .connection
        .query_row(
            "SELECT bytes FROM skill_files WHERE skill_id='proxy-pick' AND rel='scripts/pick.py'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bytes, b"piya", "字节 hex 往返应逐位一致");
}

#[test]
fn mcp_server_snapshot_strips_credentials_and_trust_and_local_rows_win() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    // 发送端：linux stdio（本机绑定命令）+ all 远程，都带鉴权头与 trusted。
    let make = |name: &str, platform: &str, transport: &str, command: &str, url: &str| {
        a.store
            .save_mcp_server(
                None,
                name,
                platform,
                transport,
                command,
                "[]",
                "{}",
                url,
                r#"{"Authorization":"Bearer SECRET"}"#,
                true,
                true,
            )
            .unwrap()
    };
    let _local = make("fs-variant", "linux", "stdio", "npx", "");
    let remote = make(
        "remote-mcp",
        "all",
        "streamable-http",
        "",
        "https://mcp.example.com/mcp",
    );
    let fresh = make(
        "fresh-remote",
        "all",
        "streamable-http",
        "",
        "https://fresh.example.com/mcp",
    );
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    // 接收端先自建同 (name,platform) 行（trusted=1、自己的 URL）——本机获胜。
    b.store
        .save_mcp_server(
            None,
            "remote-mcp",
            "all",
            "streamable-http",
            "",
            "[]",
            "{}",
            "https://my-own.example.com/mcp",
            "{}",
            true,
            true,
        )
        .unwrap();
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    // 三铁律（无本机对照的 fresh 行）：行到达、headers 不搭车（凭据不离
    // 本机）、trusted 强制 0（信任是本机事实，同步到达的 server 回到逐次
    // 审批）。
    let servers = b.store.list_mcp_servers().unwrap();
    let restored = servers.iter().find(|s| s.id == fresh).unwrap();
    assert_eq!(restored.platform, "all");
    assert_eq!(restored.headers_json, "{}");
    assert!(!restored.trusted, "trusted 不得搭车");
    // remote-mcp 被本机获胜占用：远端行不落表。
    assert!(servers.iter().all(|s| s.id != remote));
    let local_row = servers.iter().find(|s| s.name == "fs-variant").unwrap();
    assert_eq!(local_row.platform, "linux");
    assert_eq!(local_row.headers_json, "{}");
    assert!(!local_row.trusted);
    // 本机获胜：同名同平台只有一条，且是接收端自己那行。
    let mine: Vec<_> = servers.iter().filter(|s| s.name == "remote-mcp").collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].url, "https://my-own.example.com/mcp");
    assert!(mine[0].trusted, "接收端本机行不受快照影响");
    // 同步到达的行不含 headers（fresh 行的对照面已验证 trusted=0）。
    let all_synced = servers.iter().find(|s| s.id == fresh).unwrap();
    assert_eq!(all_synced.url, "https://fresh.example.com/mcp");
}
