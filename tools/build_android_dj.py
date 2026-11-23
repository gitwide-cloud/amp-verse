"""Build the shared decision-only Rust library for the Android package."""
import os,pathlib,platform,shutil,subprocess
root=pathlib.Path(__file__).resolve().parents[1];env=os.environ.copy();win=os.name=='nt'
local=root.parent/'.amp-tools'
if (local/'cargo').is_dir():
 env.setdefault('CARGO_HOME',str(local/'cargo'));env.setdefault('RUSTUP_HOME',str(local/'rustup'))
cargo=pathlib.Path(env.get('CARGO_HOME',str(pathlib.Path.home()/'.cargo')))/'bin'/('cargo.exe' if win else 'cargo')
sdk=pathlib.Path(env.get('ANDROID_HOME',env.get('ANDROID_SDK_ROOT','')))
ndk=pathlib.Path(env['ANDROID_NDK_HOME']) if env.get('ANDROID_NDK_HOME') else sdk/'ndk'/'27.0.12077973'
host='windows-x86_64' if win else 'darwin-x86_64' if platform.system()=='Darwin' else 'linux-x86_64'
bin=ndk/'toolchains/llvm/prebuilt'/host/'bin';clang=bin/('clang.exe' if win else 'clang')
if not clang.is_file():raise SystemExit('Install Android NDK 27.0.12077973 or set ANDROID_NDK_HOME to build shared DJ Logic.')
# Production remains ARM64. Only the explicit emulator job builds an additional
# x86_64 copy of this decision-only JNI library; Gradle packages it in debug only.
targets=[('aarch64-linux-android','aarch64-linux-android28','arm64-v8a')]
if env.get('AMP_UI_TEST_X86')=='1':
    targets.append(('x86_64-linux-android','x86_64-linux-android28','x86_64'))
for target,clang_target,abi in targets:
    target_env=env.copy()
    target_env.pop('RUSTFLAGS',None)
    target_env['CARGO_TARGET_'+target.upper().replace('-','_')+'_LINKER']=str(clang)
    target_env['CARGO_ENCODED_RUSTFLAGS']='\x1f'.join(['-C','link-arg=--target='+clang_target,'-C','link-arg=--sysroot='+str(bin.parent/'sysroot'),'-C','link-arg=-Wl,-z,max-page-size=16384'])
    args=[str(cargo),'build','--manifest-path',str(root/'Cargo.toml'),'-p','amp-dj-policy','--release','--target',target,'--locked']
    if env.get('AMP_CARGO_OFFLINE')=='1':args.append('--offline')
    subprocess.run(args,env=target_env,cwd=root,check=True,creationflags=0x08000000 if win else 0)
    output=root/'android/app/build/generated/djLogicJni'/abi;output.mkdir(parents=True,exist_ok=True)
    shutil.copy2(root/'target'/target/'release/libamp_dj_policy.so',output/'libamp_dj_policy.so')
