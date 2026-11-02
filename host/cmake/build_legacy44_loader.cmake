if(NOT WIN32)
    message(FATAL_ERROR "The pinned BWAPI AIModuleLoader is a Windows Win32 target")
endif()

if(DEFINED MSBUILD_COMMAND AND NOT MSBUILD_COMMAND STREQUAL "")
    set(BWRAPI_MSBUILD_EXECUTABLE "${MSBUILD_COMMAND}")
else()
    find_program(BWRAPI_MSBUILD_EXECUTABLE NAMES MSBuild.exe msbuild)
endif()
if(NOT BWRAPI_MSBUILD_EXECUTABLE)
    message(FATAL_ERROR "MSBuild.exe is required to build the pinned Win32 AIModuleLoader")
endif()

if(NOT DEFINED PLATFORM_TOOLSET OR PLATFORM_TOOLSET STREQUAL "")
    set(PLATFORM_TOOLSET "v145")
endif()

file(MAKE_DIRECTORY "${OUTPUT_DIR}")
set(_intermediate_dir "${OUTPUT_DIR}/intermediate")
file(MAKE_DIRECTORY "${_intermediate_dir}")

# Build a patched copy of the upstream loader: exception boundaries that exit
# with code 24, and lifecycle diagnostics that are on only when
# BWRAPI_HOST_DIAGNOSTICS is set in the loader's environment. The fetched
# upstream tree is not modified.
set(_staging_root "${OUTPUT_DIR}/source")
set(_staged_project_dir "${_staging_root}/AIModuleLoader")
set(_staged_source_dir "${_staged_project_dir}/Source")
file(MAKE_DIRECTORY "${_staged_source_dir}")
configure_file(
    "${UPSTREAM_ROOT}/AIModuleLoader/AIModuleLoader.vcxproj"
    "${_staged_project_dir}/AIModuleLoader.vcxproj"
    COPYONLY)
configure_file(
    "${UPSTREAM_ROOT}/AIModuleLoader/Source/AIModuleLoader.cpp"
    "${_staged_source_dir}/AIModuleLoader.cpp"
    COPYONLY)

file(READ "${_staged_source_dir}/AIModuleLoader.cpp" _loader_source)
string(REPLACE "\r\n" "\n" _loader_source "${_loader_source}")
set(_loader_original "${_loader_source}")
foreach(_anchor IN ITEMS "int main(int argc, const char* argv[])\n{" "  return 0;\n}")
    string(FIND "${_loader_source}" "${_anchor}" _offset)
    if(_offset LESS 0)
        message(FATAL_ERROR "Pinned AIModule update exception boundary anchor missing")
    endif()
endforeach()
string(REPLACE "int main(int argc, const char* argv[])\n{"
    "int main(int argc, const char* argv[])\n{\n  try {" _loader_source "${_loader_source}")
string(REPLACE "  return 0;\n}"
    "  return 0;\n  } catch (const std::exception& error) {\n    std::cerr << \"[loader-update-failed] reason=\" << error.what() << \" retry=false command_resend=false\" << std::endl;\n    ExitProcess(24);\n    return 24;\n  } catch (...) {\n    std::cerr << \"[loader-update-failed] reason=unknown exception retry=false command_resend=false\" << std::endl;\n    ExitProcess(24);\n    return 24;\n  }\n}" _loader_source "${_loader_source}")
string(REPLACE "        switch (et)" "        try {\n        switch (et)" _loader_source "${_loader_source}")
set(_callback_end [=[          default:
            break;
        }
      }]=])
set(_guarded_callback_end [=[          default:
            break;
        }
        } catch (const std::exception& error) {
          std::cerr << "[loader-callback-failed] event=" << static_cast<int>(et)
                    << " reason=" << error.what() << " retry=false command_resend=false" << std::endl;
          ExitProcess(24);
        } catch (...) {
          std::cerr << "[loader-callback-failed] event=" << static_cast<int>(et)
                    << " reason=unknown exception retry=false command_resend=false" << std::endl;
          ExitProcess(24);
        }
      }]=])
string(FIND "${_loader_source}" "${_callback_end}" _callback_end_offset)
if(_callback_end_offset LESS 0)
    message(FATAL_ERROR "Pinned AIModule callback exception boundary anchor missing")
endif()
string(REPLACE "${_callback_end}" "${_guarded_callback_end}" _loader_source "${_loader_source}")
string(REPLACE "#include <iostream>"
    "#include <iostream>\n#include <cstdlib>\n#include <cstdint>\n#include <exception>\n#include <intrin.h>\n#include <stdlib.h>"
    _loader_source "${_loader_source}")
set(_loader_diagnostic_helpers [=[
std::terminate_handler previous_terminate_handler = nullptr;
_invalid_parameter_handler previous_invalid_parameter_handler = nullptr;

void write_invalid_parameter_text(const char* label, const wchar_t* value) {
  std::cerr << label << '=';
  if (value == nullptr) {
    std::cerr << "<null>";
    return;
  }
  char text[1024] = {};
  const int length = WideCharToMultiByte(CP_UTF8, 0, value, -1, text,
                                         static_cast<int>(sizeof(text)),
                                         nullptr, nullptr);
  if (length > 0)
    std::cerr << text;
  else
    std::cerr << "<conversion-failed>";
}

void log_invalid_parameter_stack() {
  void* frames[24] = {};
  const USHORT count = CaptureStackBackTrace(0, 24, frames, nullptr);
  for (USHORT index = 0; index < count; ++index) {
    HMODULE module = nullptr;
    const DWORD flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    const BOOL has_module = GetModuleHandleExA(
        flags, reinterpret_cast<LPCSTR>(frames[index]), &module);
    const DWORD64 address = reinterpret_cast<DWORD64>(frames[index]);
    const DWORD64 base = has_module ? reinterpret_cast<DWORD64>(module) : 0;
    char path[MAX_PATH] = {};
    if (has_module) GetModuleFileNameA(module, path, MAX_PATH);
    std::cerr << "[loader-lifecycle] invalid-parameter stack[" << index
              << "]=0x" << std::hex << address << " base=0x" << base;
    if (has_module) std::cerr << " rva=0x" << (address - base);
    std::cerr << std::dec << " module=" << (has_module ? path : "<unknown>")
              << std::endl;
  }
}

void diagnostic_invalid_parameter_handler(const wchar_t* expression,
                                          const wchar_t* function,
                                          const wchar_t* file,
                                          unsigned int line, uintptr_t reserved) {
  std::cerr << "[loader-lifecycle] CRT invalid-parameter line=" << line << ' ';
  write_invalid_parameter_text("expression", expression);
  std::cerr << ' ';
  write_invalid_parameter_text("function", function);
  std::cerr << ' ';
  write_invalid_parameter_text("file", file);
  std::cerr << std::endl;
  log_invalid_parameter_stack();
  if (previous_invalid_parameter_handler != nullptr &&
      previous_invalid_parameter_handler != diagnostic_invalid_parameter_handler) {
    previous_invalid_parameter_handler(expression, function, file, line, reserved);
  } else {
    __fastfail(FAST_FAIL_INVALID_ARG);
  }
}

void diagnostic_terminate_handler() {
  std::cerr << "[loader-lifecycle] std::terminate enter" << std::endl;
  const std::exception_ptr exception = std::current_exception();
  if (exception != nullptr) {
    try {
      std::rethrow_exception(exception);
    } catch (const std::exception& error) {
      std::cerr << "[loader-lifecycle] terminate exception=" << error.what() << std::endl;
    } catch (...) {
      std::cerr << "[loader-lifecycle] terminate exception=non-std" << std::endl;
    }
  } else {
    std::cerr << "[loader-lifecycle] terminate exception=none" << std::endl;
  }
  if (previous_terminate_handler != nullptr &&
      previous_terminate_handler != diagnostic_terminate_handler)
    previous_terminate_handler();
  std::abort();
}

void destroy_loader_client(AIModule* client, HMODULE module, bool diagnostics) {
  if (client == nullptr)
    return;

  const std::uintptr_t module_base = reinterpret_cast<std::uintptr_t>(module);
  const std::uintptr_t client_address = reinterpret_cast<std::uintptr_t>(client);
  bool image_owned = false;
  if (module_base != 0) {
    const auto* dos = reinterpret_cast<const IMAGE_DOS_HEADER*>(module_base);
    if (dos->e_magic == IMAGE_DOS_SIGNATURE && dos->e_lfanew > 0) {
      const auto* nt = reinterpret_cast<const IMAGE_NT_HEADERS32*>(
          module_base + static_cast<std::uintptr_t>(dos->e_lfanew));
      if (nt->Signature == IMAGE_NT_SIGNATURE &&
          nt->OptionalHeader.Magic == IMAGE_NT_OPTIONAL_HDR32_MAGIC &&
          client_address >= module_base &&
          client_address - module_base < nt->OptionalHeader.SizeOfImage) {
        MEMORY_BASIC_INFORMATION region{};
        image_owned = VirtualQuery(client, &region, sizeof(region)) == sizeof(region) &&
                      region.State == MEM_COMMIT && region.Type == MEM_IMAGE &&
                      region.AllocationBase == module;
      }
    }
  }

  if (image_owned) {
    void** vtable = *reinterpret_cast<void***>(client);
    if (vtable == nullptr || vtable[0] == nullptr) {
      if (diagnostics)
        std::cerr << "[loader-lifecycle] module-delete invalid image vtable" << std::endl;
      ExitProcess(23);
    }
    if (diagnostics)
      std::cerr << "[loader-lifecycle] module-delete storage=image; delete-flag=0" << std::endl;
    using ScalarDeletingDestructor = void* (__thiscall *)(AIModule*, unsigned int);
    reinterpret_cast<ScalarDeletingDestructor>(vtable[0])(client, 0);
    return;
  }

  if (diagnostics)
    std::cerr << "[loader-lifecycle] module-delete storage=heap-or-unverified" << std::endl;
  delete client;
}
]=])
string(REPLACE
    "using namespace BWAPI;\n\nvoid reconnect()"
    "using namespace BWAPI;\n\n${_loader_diagnostic_helpers}\nvoid reconnect()"
    _loader_source "${_loader_source}")
string(REPLACE
    "  std::string dllPath;"
    "  const bool diagnostics = std::getenv(\"BWRAPI_HOST_DIAGNOSTICS\") != nullptr;\n  std::string dllPath;"
    _loader_source "${_loader_source}")
string(REPLACE
    "          case EventType::MatchEnd:\n            client->onEnd(e->isWinner());\n            break;"
    "          case EventType::MatchEnd:\n            if (diagnostics) {\n              std::cerr << \"[loader-lifecycle] onEnd enter winner=\" << (e->isWinner() ? \"true\" : \"false\") << std::endl;\n              previous_terminate_handler = std::set_terminate(diagnostic_terminate_handler);\n              previous_invalid_parameter_handler = _set_invalid_parameter_handler(diagnostic_invalid_parameter_handler);\n            }\n            client->onEnd(e->isWinner());\n            if (diagnostics) {\n              _set_invalid_parameter_handler(previous_invalid_parameter_handler);\n              std::set_terminate(previous_terminate_handler);\n              std::cerr << \"[loader-lifecycle] onEnd return\" << std::endl;\n            }\n            break;"
    _loader_source "${_loader_source}")
string(REPLACE
    "          case EventType::MenuFrame:\n            break;"
    "          case EventType::MenuFrame:\n            if (diagnostics) std::cerr << \"[loader-lifecycle] MenuFrame\" << std::endl;\n            break;"
    _loader_source "${_loader_source}")
string(REPLACE
    "    delete client;\n    FreeLibrary(hMod);\n    std::cout << \"Game ended\" << std::endl;"
    "    if (diagnostics) std::cerr << \"[loader-lifecycle] GameEnded enter\" << std::endl;\n    if (diagnostics) std::cerr << \"[loader-lifecycle] module-delete enter\" << std::endl;\n    destroy_loader_client(client, hMod, diagnostics);\n    if (diagnostics) std::cerr << \"[loader-lifecycle] module-delete return\" << std::endl;\n    if (diagnostics) std::cerr << \"[loader-lifecycle] FreeLibrary enter\" << std::endl;\n    FreeLibrary(hMod);\n    if (diagnostics) std::cerr << \"[loader-lifecycle] FreeLibrary return\" << std::endl;\n    std::cout << \"Game ended\" << std::endl;\n    if (diagnostics) std::cerr << \"[loader-lifecycle] GameEnded return\" << std::endl;"
    _loader_source "${_loader_source}")
if(_loader_source STREQUAL _loader_original)
    message(FATAL_ERROR "Could not instrument the pinned AIModuleLoader source")
endif()
if(NOT _loader_source MATCHES "\\[loader-lifecycle\\] onEnd enter" OR
   NOT _loader_source MATCHES "\\[loader-lifecycle\\] FreeLibrary return" OR
   NOT _loader_source MATCHES "\\[loader-lifecycle\\] MenuFrame" OR
   NOT _loader_source MATCHES "\\[loader-lifecycle\\] CRT invalid-parameter" OR
   NOT _loader_source MATCHES "\\[loader-lifecycle\\] terminate exception=" OR
   NOT _loader_source MATCHES "destroy_loader_client")
    message(FATAL_ERROR "Pinned AIModuleLoader lifecycle instrumentation is incomplete")
endif()
file(WRITE "${_staged_source_dir}/AIModuleLoader.cpp" "${_loader_source}")

file(READ "${_staged_project_dir}/AIModuleLoader.vcxproj" _loader_project)
file(TO_NATIVE_PATH "${UPSTREAM_ROOT}/include" _include_native)
include("${CMAKE_CURRENT_LIST_DIR}/stage_legacy44_client.cmake")
bwrapi_stage_legacy44_client("${UPSTREAM_ROOT}" "${_staging_root}" _client_project)
file(TO_NATIVE_PATH "${_client_project}" _client_project_native)
file(TO_NATIVE_PATH "${UPSTREAM_ROOT}/BWAPILIB/BWAPILIB.vcxproj" _lib_project_native)
string(REPLACE "../include" "${_include_native}" _loader_project "${_loader_project}")
string(REPLACE "..\\BWAPIClient\\BWAPIClient.vcxproj" "${_client_project_native}"
    _loader_project "${_loader_project}")
string(REPLACE "..\\BWAPILIB\\BWAPILIB.vcxproj" "${_lib_project_native}"
    _loader_project "${_loader_project}")
file(WRITE "${_staged_project_dir}/AIModuleLoader.vcxproj" "${_loader_project}")
set(_project "${_staged_project_dir}/AIModuleLoader.vcxproj")

# CMake keeps Windows UNC paths in slash-separated form (//server/share),
# which MSBuild parses as command-line switches.  Convert every path passed to
# MSBuild, including property values, to the native Windows spelling first.
file(TO_NATIVE_PATH "${_project}" _project_native)
file(TO_NATIVE_PATH "${OUTPUT_DIR}/" _output_dir_native)
file(TO_NATIVE_PATH "${_intermediate_dir}/" _intermediate_dir_native)

execute_process(
    COMMAND "${BWRAPI_MSBUILD_EXECUTABLE}" "${_project_native}"
        /t:Build
        /p:Configuration=Release
        /p:Platform=Win32
        "/p:PlatformToolset=${PLATFORM_TOOLSET}"
        "/p:OutDir=${_output_dir_native}"
        "/p:IntDir=${_intermediate_dir_native}"
        /p:UseStructuredOutput=false
        /m
    RESULT_VARIABLE _build_result
    OUTPUT_VARIABLE _build_output
    ERROR_VARIABLE _build_error
)
if(NOT _build_result EQUAL 0)
    message(FATAL_ERROR "Pinned AIModuleLoader build failed:\n${_build_output}\n${_build_error}")
endif()

set(_candidates
    "${OUTPUT_DIR}/AIModuleLoader.exe"
    "${OUTPUT_DIR}/Release/AIModuleLoader.exe"
)
foreach(_candidate IN LISTS _candidates)
    if(EXISTS "${_candidate}")
        execute_process(
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                "${_candidate}" "${OUTPUT_EXE}"
            RESULT_VARIABLE _copy_result
        )
        if(NOT _copy_result EQUAL 0)
            message(FATAL_ERROR "Could not stage ${_candidate} as ${OUTPUT_EXE}")
        endif()
        return()
    endif()
endforeach()
message(FATAL_ERROR "MSBuild succeeded but AIModuleLoader.exe was not found")
