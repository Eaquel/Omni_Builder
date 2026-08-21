#ifndef OMNI_BUILDER_NATIVE_BUILDER_HPP
#define OMNI_BUILDER_NATIVE_BUILDER_HPP

#include <jni.h>

#include <cstdint>

extern "C" {

uint32_t omni_abi_version(void);

const char *omni_core_version(void);

char *omni_state_report(const char *observed_environment);

char *omni_create_project(const char *root, const char *spec);

char *omni_build_project(const char *root, const char *output_path,
                         const char *key_path, const char *key_password);

char *omni_verify_self(const char *package_path,
                       const char *expected_certificate);

char *omni_create_key(const char *directory, const char *spec,
                      const char *password);

char *omni_list_keys(const char *directory);

char *omni_delete_key(const char *path);

char *omni_check_key(const char *path, const char *password);

char *omni_list_projects(const char *directory);

char *omni_project_tree(const char *root);

char *omni_read_file(const char *root, const char *relative);

char *omni_write_file(const char *root, const char *relative, const char *contents);

char *omni_new_folder(const char *root, const char *relative);

char *omni_remove_path(const char *root, const char *relative);

char *omni_set_icon(const char *root, const char *source);

void omni_string_free(char *value);

}

constexpr uint32_t kOmniExpectedAbiVersion = 1;

#endif
