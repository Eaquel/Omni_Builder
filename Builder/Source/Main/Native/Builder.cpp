#include "Builder.hpp"

#include <android/log.h>

#include <cstdint>
#include <string>

namespace {

constexpr const char *kLogTag = "OmniBuilder";
constexpr const char *kIllegalState = "java/lang/IllegalStateException";
constexpr const char *kIllegalArgument = "java/lang/IllegalArgumentException";
constexpr const char *kOutOfMemory = "java/lang/OutOfMemoryError";

void ThrowJava(JNIEnv *env, const char *klass, const char *message) {
  if (env->ExceptionCheck() == JNI_TRUE) {
    return;
  }
  jclass clazz = env->FindClass(klass);
  if (clazz == nullptr) {
    return;
  }
  env->ThrowNew(clazz, message);
  env->DeleteLocalRef(clazz);
}

void Wipe(std::string *text) {
  volatile char *bytes = const_cast<volatile char *>(text->data());
  for (size_t i = 0; i < text->size(); ++i) {
    bytes[i] = 0;
  }
  text->clear();
}

class Secret {
 public:
  Secret() = default;
  Secret(const Secret &) = delete;
  Secret &operator=(const Secret &) = delete;
  ~Secret() { Wipe(&text_); }

  std::string *buffer() { return &text_; }
  const char *c_str() const { return text_.c_str(); }

 private:
  std::string text_;
};

bool JavaStringToUtf8(JNIEnv *env, jstring value, std::string *out) {
  const jsize length = env->GetStringLength(value);
  const jchar *units = env->GetStringChars(value, nullptr);
  if (units == nullptr) {
    return false;
  }

  out->clear();
  out->reserve(static_cast<size_t>(length) + 8);

  bool ok = true;
  for (jsize i = 0; i < length && ok; ++i) {
    uint32_t code_point = units[i];

    if (code_point == 0) {
      ThrowJava(env, kIllegalArgument,
                "Argument contains a NUL character, which cannot be carried "
                "across the native boundary without truncating the value.");
      ok = false;
      break;
    }

    if (code_point >= 0xD800 && code_point <= 0xDBFF) {
      const jchar low = (i + 1 < length) ? units[i + 1] : 0;
      if (low < 0xDC00 || low > 0xDFFF) {
        ThrowJava(env, kIllegalArgument,
                  "Argument contains an unpaired UTF-16 surrogate and is not "
                  "valid text.");
        ok = false;
        break;
      }
      code_point = 0x10000 + ((code_point - 0xD800) << 10) + (low - 0xDC00);
      ++i;
    } else if (code_point >= 0xDC00 && code_point <= 0xDFFF) {
      ThrowJava(env, kIllegalArgument,
                "Argument contains an unpaired UTF-16 surrogate and is not "
                "valid text.");
      ok = false;
      break;
    }

    if (code_point < 0x80) {
      out->push_back(static_cast<char>(code_point));
    } else if (code_point < 0x800) {
      out->push_back(static_cast<char>(0xC0 | (code_point >> 6)));
      out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    } else if (code_point < 0x10000) {
      out->push_back(static_cast<char>(0xE0 | (code_point >> 12)));
      out->push_back(static_cast<char>(0x80 | ((code_point >> 6) & 0x3F)));
      out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    } else {
      out->push_back(static_cast<char>(0xF0 | (code_point >> 18)));
      out->push_back(static_cast<char>(0x80 | ((code_point >> 12) & 0x3F)));
      out->push_back(static_cast<char>(0x80 | ((code_point >> 6) & 0x3F)));
      out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    }
  }

  env->ReleaseStringChars(value, units);
  return ok;
}

bool JavaCharsToUtf8(JNIEnv *env, jcharArray value, std::string *out) {
  const jsize length = env->GetArrayLength(value);
  jchar *units = env->GetCharArrayElements(value, nullptr);
  if (units == nullptr) {
    return false;
  }

  out->clear();
  out->reserve(static_cast<size_t>(length) + 8);

  bool ok = true;
  for (jsize i = 0; i < length && ok; ++i) {
    uint32_t code_point = units[i];

    if (code_point >= 0xD800 && code_point <= 0xDBFF) {
      const jchar low = (i + 1 < length) ? units[i + 1] : 0;
      if (low < 0xDC00 || low > 0xDFFF) {
        ok = false;
        break;
      }
      code_point = 0x10000 + ((code_point - 0xD800) << 10) + (low - 0xDC00);
      ++i;
    } else if (code_point >= 0xDC00 && code_point <= 0xDFFF) {
      ok = false;
      break;
    }

    if (code_point < 0x80) {
      out->push_back(static_cast<char>(code_point));
    } else if (code_point < 0x800) {
      out->push_back(static_cast<char>(0xC0 | (code_point >> 6)));
      out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    } else if (code_point < 0x10000) {
      out->push_back(static_cast<char>(0xE0 | (code_point >> 12)));
      out->push_back(static_cast<char>(0x80 | ((code_point >> 6) & 0x3F)));
      out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    } else {
      out->push_back(static_cast<char>(0xF0 | (code_point >> 18)));
      out->push_back(static_cast<char>(0x80 | ((code_point >> 12) & 0x3F)));
      out->push_back(static_cast<char>(0x80 | ((code_point >> 6) & 0x3F)));
      out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    }
  }

  for (jsize i = 0; i < length; ++i) {
    units[i] = 0;
  }
  env->ReleaseCharArrayElements(value, units, 0);

  if (!ok) {
    ThrowJava(env, kIllegalArgument,
              "The password contains an unpaired UTF-16 surrogate and is not "
              "valid text.");
    Wipe(out);
  }
  return ok;
}

jstring CallCoreReport(JNIEnv *env, const char *observed) {
  char *report = omni_state_report(observed);
  if (report == nullptr) {
    ThrowJava(env, kIllegalState,
              "Omni Core could not produce a state report. This is a defect in "
              "the Core, not in the project being built.");
    return nullptr;
  }

  jstring result = env->NewStringUTF(report);
  omni_string_free(report);

  if (result == nullptr) {
    ThrowJava(env, kOutOfMemory,
              "Omni Core state report could not be converted to a Java string.");
    return nullptr;
  }
  return result;
}

}

extern "C" {

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void * ) {
  JNIEnv *env = nullptr;
  if (vm->GetEnv(reinterpret_cast<void **>(&env), JNI_VERSION_1_6) != JNI_OK) {
    __android_log_print(ANDROID_LOG_ERROR, kLogTag,
                        "JNI 1.6 is not available; refusing to load.");
    return JNI_ERR;
  }

  const uint32_t core_abi = omni_abi_version();
  if (core_abi != kOmniExpectedAbiVersion) {
    __android_log_print(
        ANDROID_LOG_ERROR, kLogTag,
        "ABI mismatch: bridge expects %u, Core provides %u. Refusing to load.",
        kOmniExpectedAbiVersion, core_abi);
    return JNI_ERR;
  }

  __android_log_print(ANDROID_LOG_INFO, kLogTag,
                      "Omni Core %s loaded (ABI %u).", omni_core_version(),
                      core_abi);
  return JNI_VERSION_1_6;
}

JNIEXPORT jint JNICALL Java_com_omni_builder_Builder_nativeAbiVersion(
    JNIEnv * , jobject ) {
  return static_cast<jint>(omni_abi_version());
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeCoreVersion(
    JNIEnv *env, jobject ) {
  const char *version = omni_core_version();
  if (version == nullptr) {
    ThrowJava(env, kIllegalState, "Omni Core reported no version.");
    return nullptr;
  }
  jstring result = env->NewStringUTF(version);
  if (result == nullptr) {
    ThrowJava(env, kOutOfMemory, "Omni Core version could not be converted.");
  }
  return result;
}

jstring HandBack(JNIEnv *env, char *report) {
  if (report == nullptr) {
    ThrowJava(env, kIllegalState,
              "Omni Core could not report. This is a defect in the Core, not "
              "in the project being built.");
    return nullptr;
  }
  jstring result = env->NewStringUTF(report);
  omni_string_free(report);
  if (result == nullptr) {
    ThrowJava(env, kOutOfMemory,
              "A Core report could not be converted to a Java string.");
  }
  return result;
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeCreateProject(
    JNIEnv *env, jobject , jstring root, jstring spec) {
  std::string root_text;
  std::string spec_text;
  if (root == nullptr || spec == nullptr ||
      !JavaStringToUtf8(env, root, &root_text) ||
      !JavaStringToUtf8(env, spec, &spec_text)) {
    ThrowJava(env, kIllegalState, "A project needs a folder and a specification.");
    return nullptr;
  }
  return HandBack(env, omni_create_project(root_text.c_str(), spec_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeBuildAll(
    JNIEnv *env, jobject , jstring root, jstring package_path,
    jstring bundle_path, jstring key_path, jcharArray key_password) {
  std::string root_text;
  std::string package_text;
  std::string bundle_text;
  std::string key_text;
  if (root == nullptr || package_path == nullptr || bundle_path == nullptr ||
      key_path == nullptr || !JavaStringToUtf8(env, root, &root_text) ||
      !JavaStringToUtf8(env, package_path, &package_text) ||
      !JavaStringToUtf8(env, bundle_path, &bundle_text) ||
      !JavaStringToUtf8(env, key_path, &key_text)) {
    ThrowJava(env, kIllegalState,
              "A build needs a project, a package, a bundle and a key.");
    return nullptr;
  }

  Secret password;
  if (key_password != nullptr &&
      !JavaCharsToUtf8(env, key_password, password.buffer())) {
    return nullptr;
  }

  return HandBack(env, omni_build_all(root_text.c_str(), package_text.c_str(),
                                      bundle_text.c_str(), key_text.c_str(),
                                      key_password == nullptr
                                          ? nullptr
                                          : password.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeBindDevice(
    JNIEnv *env, jobject , jstring secret) {
  std::string secret_text;
  if (secret == nullptr || !JavaStringToUtf8(env, secret, &secret_text)) {
    ThrowJava(env, kIllegalState, "Binding the shared key needs something to bind it to.");
    return nullptr;
  }
  jstring answer = HandBack(env, omni_bind_device(secret_text.c_str()));
  // What was handed over is not wanted after this call. The Core has mixed it
  // into what it needs; this copy is wiped rather than left in the heap for
  // whatever reads it next.
  Wipe(&secret_text);
  return answer;
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeDefaultKey(
    JNIEnv *env, jobject , jstring directory) {
  std::string directory_text;
  if (directory == nullptr || !JavaStringToUtf8(env, directory, &directory_text)) {
    ThrowJava(env, kIllegalState, "The shared signing key needs a folder.");
    return nullptr;
  }
  return HandBack(env, omni_default_key(directory_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeCreateKey(
    JNIEnv *env, jobject , jstring directory, jstring spec,
    jcharArray key_password) {
  std::string directory_text;
  std::string spec_text;
  if (directory == nullptr || spec == nullptr || key_password == nullptr ||
      !JavaStringToUtf8(env, directory, &directory_text) ||
      !JavaStringToUtf8(env, spec, &spec_text)) {
    ThrowJava(env, kIllegalState,
              "A signing key needs a folder, a specification and a password.");
    return nullptr;
  }

  Secret password;
  if (!JavaCharsToUtf8(env, key_password, password.buffer())) {
    return nullptr;
  }

  return HandBack(env, omni_create_key(directory_text.c_str(), spec_text.c_str(),
                                       password.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeListKeys(
    JNIEnv *env, jobject , jstring directory) {
  std::string directory_text;
  if (directory == nullptr || !JavaStringToUtf8(env, directory, &directory_text)) {
    ThrowJava(env, kIllegalState, "Listing signing keys needs a folder.");
    return nullptr;
  }
  return HandBack(env, omni_list_keys(directory_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeDeleteKey(
    JNIEnv *env, jobject , jstring path) {
  std::string path_text;
  if (path == nullptr || !JavaStringToUtf8(env, path, &path_text)) {
    ThrowJava(env, kIllegalState, "Removing a signing key needs its path.");
    return nullptr;
  }
  return HandBack(env, omni_delete_key(path_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeCheckKey(
    JNIEnv *env, jobject , jstring path, jcharArray key_password) {
  std::string path_text;
  if (path == nullptr || key_password == nullptr ||
      !JavaStringToUtf8(env, path, &path_text)) {
    ThrowJava(env, kIllegalState, "Opening a signing key needs its path and password.");
    return nullptr;
  }

  Secret password;
  if (!JavaCharsToUtf8(env, key_password, password.buffer())) {
    return nullptr;
  }

  return HandBack(env, omni_check_key(path_text.c_str(), password.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeVerifySelf(
    JNIEnv *env, jobject , jstring package_path, jstring expected) {
  std::string path_text;
  std::string expected_text;
  if (package_path == nullptr || !JavaStringToUtf8(env, package_path, &path_text)) {
    ThrowJava(env, kIllegalState, "A verification needs a package path.");
    return nullptr;
  }
  if (expected != nullptr && !JavaStringToUtf8(env, expected, &expected_text)) {
    return nullptr;
  }
  return HandBack(env, omni_verify_self(path_text.c_str(),
                                        expected == nullptr ? nullptr
                                                            : expected_text.c_str()));
}

bool TwoPaths(JNIEnv *env, jstring first, jstring second, std::string *first_text,
              std::string *second_text, const char *what) {
  if (first == nullptr || second == nullptr ||
      !JavaStringToUtf8(env, first, first_text) ||
      !JavaStringToUtf8(env, second, second_text)) {
    ThrowJava(env, kIllegalState, what);
    return false;
  }
  return true;
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeListProjects(
    JNIEnv *env, jobject , jstring directory) {
  std::string directory_text;
  if (directory == nullptr || !JavaStringToUtf8(env, directory, &directory_text)) {
    ThrowJava(env, kIllegalState, "Listing projects needs a folder.");
    return nullptr;
  }
  return HandBack(env, omni_list_projects(directory_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeProjectTree(
    JNIEnv *env, jobject , jstring root) {
  std::string root_text;
  if (root == nullptr || !JavaStringToUtf8(env, root, &root_text)) {
    ThrowJava(env, kIllegalState, "Reading a project needs its folder.");
    return nullptr;
  }
  return HandBack(env, omni_project_tree(root_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeSymbols(
    JNIEnv *env, jobject , jstring root, jstring needle) {
  std::string root_text;
  std::string needle_text;
  if (root == nullptr || !JavaStringToUtf8(env, root, &root_text) ||
      needle == nullptr || !JavaStringToUtf8(env, needle, &needle_text)) {
    ThrowJava(env, kIllegalState, "Looking for a name needs a project and a name.");
    return nullptr;
  }
  return HandBack(env, omni_symbols(root_text.c_str(), needle_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeWhereWritten(
    JNIEnv *env, jobject , jstring root, jstring qualified) {
  std::string root_text;
  std::string qualified_text;
  if (root == nullptr || !JavaStringToUtf8(env, root, &root_text) ||
      qualified == nullptr || !JavaStringToUtf8(env, qualified, &qualified_text)) {
    ThrowJava(env, kIllegalState, "Finding where a type is written needs a project and a type.");
    return nullptr;
  }
  return HandBack(env, omni_where_written(root_text.c_str(), qualified_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeInspectPackage(
    JNIEnv *env, jobject , jstring path) {
  std::string path_text;
  if (path == nullptr || !JavaStringToUtf8(env, path, &path_text)) {
    ThrowJava(env, kIllegalState, "Opening a package needs its path.");
    return nullptr;
  }
  return HandBack(env, omni_inspect_package(path_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeCheckProject(
    JNIEnv *env, jobject , jstring root) {
  std::string root_text;
  if (root == nullptr || !JavaStringToUtf8(env, root, &root_text)) {
    ThrowJava(env, kIllegalState, "Checking a project needs its folder.");
    return nullptr;
  }
  return HandBack(env, omni_check_project(root_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeSearchProject(
    JNIEnv *env, jobject , jstring root, jstring needle, jboolean sensitive,
    jboolean whole_word) {
  std::string root_text;
  std::string needle_text;
  if (root == nullptr || !JavaStringToUtf8(env, root, &root_text) ||
      needle == nullptr || !JavaStringToUtf8(env, needle, &needle_text)) {
    ThrowJava(env, kIllegalState, "Searching needs a project and something to look for.");
    return nullptr;
  }
  return HandBack(env, omni_search_project(root_text.c_str(), needle_text.c_str(),
                                           sensitive == JNI_TRUE,
                                           whole_word == JNI_TRUE));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeReadFile(
    JNIEnv *env, jobject , jstring root, jstring relative) {
  std::string root_text;
  std::string relative_text;
  if (!TwoPaths(env, root, relative, &root_text, &relative_text,
                "Reading a file needs a project and a path.")) {
    return nullptr;
  }
  return HandBack(env, omni_read_file(root_text.c_str(), relative_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeWriteFile(
    JNIEnv *env, jobject , jstring root, jstring relative, jstring contents) {
  std::string root_text;
  std::string relative_text;
  std::string contents_text;
  if (!TwoPaths(env, root, relative, &root_text, &relative_text,
                "Saving a file needs a project and a path.")) {
    return nullptr;
  }
  if (contents != nullptr && !JavaStringToUtf8(env, contents, &contents_text)) {
    return nullptr;
  }
  return HandBack(env, omni_write_file(root_text.c_str(), relative_text.c_str(),
                                       contents_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeNewFolder(
    JNIEnv *env, jobject , jstring root, jstring relative) {
  std::string root_text;
  std::string relative_text;
  if (!TwoPaths(env, root, relative, &root_text, &relative_text,
                "Making a folder needs a project and a path.")) {
    return nullptr;
  }
  return HandBack(env, omni_new_folder(root_text.c_str(), relative_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeRemovePath(
    JNIEnv *env, jobject , jstring root, jstring relative, jstring trash_root) {
  std::string root_text;
  std::string relative_text;
  std::string trash_text;
  if (!TwoPaths(env, root, relative, &root_text, &relative_text,
                "Removing something needs a project, a path and a trash.")) {
    return nullptr;
  }
  if (trash_root == nullptr || !JavaStringToUtf8(env, trash_root, &trash_text)) {
    ThrowJava(env, kIllegalState,
              "Removing something needs a project, a path and a trash.");
    return nullptr;
  }
  return HandBack(env, omni_remove_path(root_text.c_str(), relative_text.c_str(),
                                        trash_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeRenamePath(
    JNIEnv *env, jobject , jstring root, jstring from, jstring to) {
  std::string root_text;
  std::string from_text;
  std::string to_text;
  if (!TwoPaths(env, root, from, &root_text, &from_text,
                "Moving something needs a project and two paths.")) {
    return nullptr;
  }
  if (to == nullptr || !JavaStringToUtf8(env, to, &to_text)) {
    ThrowJava(env, kIllegalState,
              "Moving something needs a project and two paths.");
    return nullptr;
  }
  return HandBack(env, omni_rename_path(root_text.c_str(), from_text.c_str(),
                                        to_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeListBuilt(
    JNIEnv *env, jobject , jstring directory) {
  std::string directory_text;
  if (directory == nullptr || !JavaStringToUtf8(env, directory, &directory_text)) {
    ThrowJava(env, kIllegalState, "Listing what was built needs a folder.");
    return nullptr;
  }
  return HandBack(env, omni_list_built(directory_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeTrashSend(
    JNIEnv *env, jobject , jstring trash_root, jstring path) {
  std::string trash_text;
  std::string path_text;
  if (!TwoPaths(env, trash_root, path, &trash_text, &path_text,
                "Deleting something needs a trash and a path.")) {
    return nullptr;
  }
  return HandBack(env, omni_trash_send(trash_text.c_str(), path_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeTrashList(
    JNIEnv *env, jobject , jstring trash_root) {
  std::string trash_text;
  if (trash_root == nullptr || !JavaStringToUtf8(env, trash_root, &trash_text)) {
    ThrowJava(env, kIllegalState, "Listing the trash needs its folder.");
    return nullptr;
  }
  return HandBack(env, omni_trash_list(trash_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeTrashRestore(
    JNIEnv *env, jobject , jstring trash_root, jstring id) {
  std::string trash_text;
  std::string id_text;
  if (!TwoPaths(env, trash_root, id, &trash_text, &id_text,
                "Putting something back needs a trash and a name.")) {
    return nullptr;
  }
  return HandBack(env, omni_trash_restore(trash_text.c_str(), id_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeTrashPurge(
    JNIEnv *env, jobject , jstring trash_root, jstring id) {
  std::string trash_text;
  std::string id_text;
  if (!TwoPaths(env, trash_root, id, &trash_text, &id_text,
                "Removing something for good needs a trash and a name.")) {
    return nullptr;
  }
  return HandBack(env, omni_trash_purge(trash_text.c_str(), id_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeTrashEmpty(
    JNIEnv *env, jobject , jstring trash_root) {
  std::string trash_text;
  if (trash_root == nullptr || !JavaStringToUtf8(env, trash_root, &trash_text)) {
    ThrowJava(env, kIllegalState, "Emptying the trash needs its folder.");
    return nullptr;
  }
  return HandBack(env, omni_trash_empty(trash_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeSetIcon(
    JNIEnv *env, jobject , jstring root, jstring source) {
  std::string root_text;
  std::string source_text;
  if (!TwoPaths(env, root, source, &root_text, &source_text,
                "An application image needs a project and a file.")) {
    return nullptr;
  }
  return HandBack(env, omni_set_icon(root_text.c_str(), source_text.c_str()));
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeBuildProgress(
    JNIEnv *env, jobject ) {
  return HandBack(env, omni_build_progress());
}

JNIEXPORT void JNICALL Java_com_omni_builder_Builder_nativeBuildExpect(
    JNIEnv *env, jobject , jstring timings) {
  if (timings == nullptr) {
    omni_build_expect(nullptr);
    return;
  }
  std::string held;
  if (!JavaStringToUtf8(env, timings, &held)) {
    return;
  }
  omni_build_expect(held.c_str());
}

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeStateReport(
    JNIEnv *env, jobject , jstring observed_environment) {
  if (observed_environment == nullptr) {
    return CallCoreReport(env, nullptr);
  }

  std::string observed;
  if (!JavaStringToUtf8(env, observed_environment, &observed)) {
    return nullptr;
  }

  return CallCoreReport(env, observed.c_str());
}

}
