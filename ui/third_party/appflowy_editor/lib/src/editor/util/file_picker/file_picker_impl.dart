import 'package:appflowy_editor/src/editor/util/file_picker/file_picker_service.dart';
import 'package:file_picker/file_picker.dart' as fp;

class FilePicker implements FilePickerService {
  @override
  Future<String?> getDirectoryPath({String? title}) {
    return fp.FilePicker.getDirectoryPath(dialogTitle: title);
  }

  @override
  Future<FilePickerResult?> pickFiles({
    String? dialogTitle,
    String? initialDirectory,
    fp.FileType type = fp.FileType.any,
    List<String>? allowedExtensions,
    Function(fp.FilePickerStatus p1)? onFileLoading,
    bool allowMultiple = false,
    bool withData = false,
    bool withReadStream = false,
    bool lockParentWindow = false,
  }) async {
    // 本地补丁（file_picker 13）：API 从 FilePicker.platform.* 改为静态方法；
    // withData / withReadStream / lockParentWindow 参数已移除（文件内容改为
    // 按需 PlatformFile.readAsBytes() / readAsByteStream() 读取），此处忽略；
    // pickFiles 恒为多选语义，取消返回空列表（旧版返回 null）。
    final files = await fp.FilePicker.pickFiles(
      dialogTitle: dialogTitle,
      initialDirectory: initialDirectory,
      type: type,
      allowedExtensions: allowedExtensions,
      onFileLoading: onFileLoading,
    );

    if (files.isEmpty) {
      return null;
    }
    return FilePickerResult(allowMultiple ? files : [files.first]);
  }

  // saveFile 不提供实现：file_picker 13 的 saveFile 改为「传入字节由插件代存」
  // （required fileName + bytes），与旧的「选择保存路径」语义不兼容；
  // 编辑器内部无调用点，保持 UnimplementedError。
  @override
  Future<String?> saveFile({
    String? dialogTitle,
    String? fileName,
    String? initialDirectory,
    fp.FileType type = fp.FileType.any,
    List<String>? allowedExtensions,
    bool lockParentWindow = false,
  }) async =>
      throw UnimplementedError('saveFile() has not been implemented.');
}
