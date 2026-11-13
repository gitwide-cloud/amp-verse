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
env.pop('RUSTFLAGS',None);env['CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER']=str(clang)
env['CARGO_ENCODED_RUSTFLAGS']='\x1f'.join(['-C','link-arg=--target=aarch64-linux-android28','-C','link-arg=--sysroot='+str(bin.parent/'sysroot'),'-C','link-arg=-Wl,-z,max-page-size=16384'])
args=[str(cargo),'build','--manifest-path',str(root/'Cargo.toml'),'-p','amp-dj-policy','--release','--target','aarch64-linux-android','--locked']
if env.get('AMP_CARGO_OFFLINE')=='1':args.append('--offline')
subprocess.run(args,env=env,cwd=root,check=True,creationflags=0x08000000 if win else 0)
output=root/'android/app/build/generated/djLogicJni/arm64-v8a';output.mkdir(parents=True,exist_ok=True)
shutil.copy2(root/'target/aarch64-linux-android/release/libamp_dj_policy.so',output/'libamp_dj_policy.so')
