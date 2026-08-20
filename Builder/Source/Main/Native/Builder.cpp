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

JNIEXPORT jstring JNICALL Java_com_omni_builder_Builder_nativeBuildProject(
    JNIEnv *env, jobject , jstring root, jstring output_path, jstring key_path,
    jcharArray key_password) {
  std::string root_text;
  std::string path_text;
  std::string key_text;
  if (root == nullptr || output_path == nullptr || key_path == nullptr ||
      !JavaStringToUtf8(env, root, &root_text) ||
      !JavaStringToUtf8(env, output_path, &path_text) ||
      !JavaStringToUtf8(env, key_path, &key_text)) {
    ThrowJava(env, kIllegalState, "A build needs a project, an output and a key.");
    return nullptr;
  }

  Secret password;
  if (key_password != nullptr &&
      !JavaCharsToUtf8(env, key_password, password.buffer())) {
    return nullptr;
  }

  return HandBack(env, omni_build_project(root_text.c_str(), path_text.c_str(),
                                          key_text.c_str(),
                                          key_password == nullptr
                                              ? nullptr
                                              : password.c_str()));
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
