# Stage the complete Client project with one source patch. Both loaders and
# the installed Client library are built from this stage.
function(bwrapi_stage_legacy44_client upstream stage output_project)
    # The patch anchor below fails the build if the upstream source changes shape.
    file(MAKE_DIRECTORY "${stage}")
    file(COPY "${upstream}/BWAPIClient" DESTINATION "${stage}")
    set(_dir "${stage}/BWAPIClient")
    file(READ "${_dir}/Source/GameImpl.cpp" _game)
    string(REPLACE "\r\n" "\n" _game "${_game}")
    # The loader unloads the bot module after a match; events the bot
    # registered on Game must go with it, as official in-game BWAPI does.
    set(_old [=[  void GameImpl::onMatchEnd()
  {
    clearAll();]=])
    set(_new [=[  void GameImpl::onMatchEnd()
  {
    this->interfaceEvents.clear();
    clearAll();]=])
    string(FIND "${_game}" "${_old}" _offset)
    if(_offset LESS 0)
        message(FATAL_ERROR "Pinned match end event patch anchor missing")
    endif()
    string(REPLACE "${_old}" "${_new}" _game "${_game}")
    file(WRITE "${_dir}/Source/GameImpl.cpp" "${_game}")
    file(READ "${_dir}/BWAPIClient.vcxproj" _project)
    foreach(_part IN ITEMS include Shared)
        file(TO_NATIVE_PATH "${upstream}/${_part}" _native)
        string(REPLACE "../${_part}" "${_native}" _project "${_project}")
        string(REPLACE "..\\${_part}" "${_native}" _project "${_project}")
    endforeach()
    # Build the Client for the Vista Windows API level.
    string(REPLACE "_WIN32_WINNT=0x0501" "_WIN32_WINNT=0x0600" _project "${_project}")
    string(REPLACE "NTDDI_VERSION=0x05010300" "NTDDI_VERSION=0x06000000" _project "${_project}")
    file(WRITE "${_dir}/BWAPIClient.vcxproj" "${_project}")
    set(${output_project} "${_dir}/BWAPIClient.vcxproj" PARENT_SCOPE)
endfunction()
