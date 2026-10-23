// 锁屏交互契约：4 格输满即自动校验（无按钮）；输错清空并提示；
// 非 4 位旧密码走兜底入口按回车解锁。使用注入的假服务，无真实安全存储。
import "package:flutter/material.dart";
import "package:flutter_secure_storage/flutter_secure_storage.dart";
import "package:flutter_test/flutter_test.dart";
import "package:elsewhen_ui/screens/lock_screen.dart";
import "package:elsewhen_ui/utils/app_lock.dart";

class _FakeAppLockService extends AppLockService {
  _FakeAppLockService(this.password) : super(storage: const _NoStorage());
  final String password;

  @override
  Future<bool> verifyPassword(String input) async => input == password;

  @override
  Future<bool> biometricUnlockEnabled() async => false;

  @override
  Future<bool> biometricAvailable() async => false;
}

/// 存储层永不触碰：任何读到都意味着假服务漏了覆写（测试环境无平台通道）。
class _NoStorage implements FlutterSecureStorage {
  const _NoStorage();
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError("fake service must not touch secure storage");
}

/// 解锁成功后立即离开锁屏（与真实宿主语义一致），避免校验期 spinner
/// 在测试里常驻导致 pumpAndSettle 永不收敛。
class _Host extends StatefulWidget {
  const _Host({this.password = "9999"});
  final String password;
  @override
  State<_Host> createState() => _HostState();
}

class _HostState extends State<_Host> {
  bool _unlocked = false;

  @override
  Widget build(BuildContext context) {
    if (_unlocked) return const Scaffold(body: Center(child: Text("unlocked")));
    return LockScreen(
      onUnlocked: () => setState(() => _unlocked = true),
      service: _FakeAppLockService(widget.password),
    );
  }
}

void main() {
  testWidgets("输满 4 位自动解锁，没有解锁按钮", (tester) async {
    await tester.pumpWidget(const MaterialApp(home: _Host(password: "1234")));

    expect(find.text("解锁"), findsNothing, reason: "没有解锁按钮");

    await tester.enterText(find.byType(TextField), "1234");
    await tester.pumpAndSettle();

    expect(find.text("unlocked"), findsOneWidget, reason: "4 位输满应立即自动校验");
  });

  testWidgets("错误密码清空并提示", (tester) async {
    await tester.pumpWidget(const MaterialApp(home: _Host(password: "9999")));
    await tester.enterText(find.byType(TextField), "1234");
    await tester.pumpAndSettle();

    expect(find.text("unlocked"), findsNothing);
    expect(find.text("密码不正确"), findsOneWidget);
    // 输错后格子清空（隐藏输入层也随之复位，可再输）。
    await tester.enterText(find.byType(TextField), "9999");
    await tester.pumpAndSettle();
    expect(find.text("unlocked"), findsOneWidget, reason: "清空后可继续输入解锁");
  });

  testWidgets("兜底入口：非 4 位旧密码按回车解锁", (tester) async {
    await tester.pumpWidget(
      const MaterialApp(home: _Host(password: "old-password")),
    );
    await tester.tap(find.text("旧密码不是 4 位？换输入方式"));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), "old-password");
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();

    expect(find.text("unlocked"), findsOneWidget);
  });
}
