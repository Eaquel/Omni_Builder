#ifndef OMNI_BUILDER_NATIVE_BUILDER_HPP
#define OMNI_BUILDER_NATIVE_BUILDER_HPP

#include <jni.h>

#include <cstdint>

extern "C" {

uint32_t omni_abi_version(void);

const char *omni_core_version(void);

char *omni_state_report(const char *observed_environment);

char *omni_create_project(const char *root, const char *spec);

char *omni_build_all(const char *root, const char *package_path,
                     const char *bundle_path, const char *key_path,
                     const char *key_password);

char *omni_build_progress(void);

void omni_build_expect(const char *timings);

char *omni_verify_self(const char *package_path,
                       const char *expected_certificate);

char *omni_create_key(const char *directory, const char *spec,
                      const char *password);

char *omni_bind_device(const char *secret);
char *omni_default_key(const char *directory);

char *omni_list_keys(const char *directory);

char *omni_delete_key(const char *path);

char *omni_check_key(const char *path, const char *password);

char *omni_list_projects(const char *directory);

char *omni_project_tree(const char *root);

char *omni_search_project(const char *root, const char *needle, bool sensitive,
                          bool whole_word);

char *omni_read_file(const char *root, const char *relative);

char *omni_write_file(const char *root, const char *relative, const char *contents);

char *omni_new_folder(const char *root, const char *relative);

char *omni_remove_path(const char *root, const char *relative,
                       const char *trash_root);

char *omni_rename_path(const char *root, const char *from, const char *to);

char *omni_list_built(const char *directory);

char *omni_trash_send(const char *trash_root, const char *path);

char *omni_trash_list(const char *trash_root);

char *omni_trash_restore(const char *trash_root, const char *id);

char *omni_trash_purge(const char *trash_root, const char *id);

char *omni_trash_empty(const char *trash_root);

char *omni_set_icon(const char *root, const char *source);

void omni_string_free(char *value);

}

constexpr uint32_t kOmniExpectedAbiVersion = 1;

#endif
