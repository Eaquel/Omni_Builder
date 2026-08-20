#ifndef OMNI_BUILDER_NATIVE_BUILDER_HPP
#define OMNI_BUILDER_NATIVE_BUILDER_HPP

#include <jni.h>

#include <cstdint>

extern "C" {

uint32_t omni_abi_version(void);

const char *omni_core_version(void);

char *omni_state_report(const char *observed_environment);

char *omni_build_package(const char *manifest, const char *output_path);

void omni_string_free(char *value);

}

constexpr uint32_t kOmniExpectedAbiVersion = 1;

#endif
