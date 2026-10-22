# Forge、Fabric 与 NeoForge

## 使用

1. 在「设置」选择下载源，并确认 Java 路径。留空会自动寻找与 Minecraft 主版本及本机架构匹配的 Java。
2. 打开「下载 → Minecraft」，搜索游戏版本并点击「选择版本」，再选择 Forge、Fabric 或 NeoForge，列表只显示该游戏版本兼容的加载器。选择加载器版本不会立即下载，切换游戏版本或加载器后需重新选择；不需要加载器时选择「不安装加载器」。列表支持搜索、刷新，预览版本会单独标记；没有兼容版本时可重试或安装原版。
3. 确认底部的游戏版本与加载器，点击「开始安装」。启动器先安装或校验原版文件，再安装加载器依赖。Forge / NeoForge 还会执行官方安装清单中的客户端补丁处理器。
4. 完成后在「开始」选择新实例，使用正版账号启动；离线测试须明确选择对应身份。实例的模组、配置和存档位于独立目录。
5. 已安装版本可以「校验 / 修复」。重新安装同一版本不会清空实例目录。切换加载器或版本会创建另一个独立版本，不迁移或覆盖现有存档。

Fabric API、Mod 下载和整合包仍是独立功能，本流程只安装加载器。

## 安装与启动

Fabric 从官方 Meta V2 查询兼容列表与启动配置，依赖从 Fabric Maven 获取，优先使用元数据校验值，缺少时读取 Maven 的 `.sha1`。

Forge 从官方 Maven 列表筛选完整的 Minecraft 前缀。NeoForge 区分 `1.20.1` 的旧 `net.neoforged:forge`、后续 `net.neoforged:neoforge`，并处理 `21.1.x → 1.21.1`、`26.3.0.x → 26.3` 等命名。安装时再次核对包内 Minecraft 版本和父版本，不以字符串模糊匹配决定兼容性。

Forge / NeoForge 下载并校验官方 installer jar，读取 `install_profile.json` 与版本配置，解压必要 Maven 库，再执行仅适用于客户端的 Java 处理器。服务端处理器不会执行。Mojang 客户端映射由启动器下载并校验，不交由 Java 处理器另行选择镜像。支持包内 Maven 库和旧安装包的 universal jar；具体历史安装包仍可能存在未覆盖的格式。

补丁文件按当前客户端处理步骤声明的 `outputs` 校验 SHA-1；全部校验通过的缓存可直接复用，只有需要重新执行 Java 补丁处理器时才检查其 Java 环境。安装清单 `data` 中未被客户端使用的变量不视为必须生成的文件，例如新版 Forge 客户端直接修补原版 JAR，不要求生成服务端步骤使用的 `MC_UNPACKED` 路径。实际生成文件仍记录校验值，启动前继续检查损坏情况。

启动器在安装时合并父版本资源、JVM 参数、游戏参数与加载器入口，并按 Maven 组件和 classifier 替换冲突依赖。原版资源与依赖使用共享缓存；为满足 Forge 的 jar 排除规则，版本目录中另放一份对应原版启动 jar。加载器补丁输出和依赖的校验值记录在安装标记中，启动前复核，缺失或损坏时提示修复。

安装标记最后写入。取消、校验失败、补丁异常或超时不会将未完成版本发布为可启动实例；已完成的原版依赖与下载缓存可在重试时复用。修复期间会先撤销该加载器的完成标记，成功后重新写入，存档不会删除。

## Java 与日志

加载器本身与不同 Minecraft 版本需要兼容的 Java。示例：Minecraft 1.20.1 使用 Java 17，1.21.1 使用 Java 21，26.3 使用其官方元数据要求的 Java。自动匹配不使用任意更高版本替代所需主版本；没有合适 Java 会显示安装或配置提示。Fabric 安装本身不执行 Java，启动时仍检查 Java。

Forge / NeoForge 补丁输出保存在游戏文件目录的 `.nscl/installers/<实例版本 ID>.log`，下载缓存位于同目录的 `.jar`。每个 Java 处理步骤有超时与取消检查，取消时终止当前处理器。失败提示包含具体日志路径。游戏日志继续保存在 `instances/<实例 ID>/logs/nscl-launch.log`。

启动器代码与界面全部使用 Rust；执行加载器提供的 Java 补丁工具不依赖 WebView、JavaScript 或官方 Minecraft 启动器。

## Fabric 首次启动与 Windows 权限

Fabric 首次启动时会在实例的 `.fabric/remappedJars/` 下生成映射缓存。若日志出现 `client-intermediary.jar.tmp can't be written`，可能是实例目录确实不可写，也可能是启动器处于 Windows 的低完整性权限环境。后者会让子进程 Java 的 `Files.isWritable` 返回 `false`，即使它实际能在低完整性目录创建文件，ZIP 文件系统仍会进入只读模式。

NSCL 会识别 Fabric 映射 JAR 写入失败的日志；在 Windows 上结合当前进程的完整性级别区分低完整性环境与普通目录写入问题，并在界面和 CLI 中显示恢复办法及原始日志路径。检测只在对应启动失败后进行，不阻止已有缓存的正常启动；它只读取当前进程令牌，不修改系统权限或目录 ACL。

遇到此提示，请退出 NSCL，将启动器 EXE **复制**到普通用户目录后重新打开。目标目录应位于沙箱或受限开发目录之外；仅重新运行原 EXE 或在同一个受限目录内复制文件，仍可能保留或继承低完整性标签。保留原有游戏目录设置即可，无需重装游戏或删除存档，也不需要管理员权限。若已在普通桌面环境运行但仍出现 JAR 写入错误，再检查实例目录的写入权限、磁盘空间和安全软件拦截情况。

排查原理可参考 [Windows 强制完整性控制](https://learn.microsoft.com/en-us/windows/win32/secauthz/mandatory-integrity-control)与 [OpenJDK Windows 权限预检实现](https://github.com/openjdk/jdk17u/blob/master/src/java.base/windows/classes/sun/nio/fs/WindowsSecurity.java)。

## 镜像

沿用「设置 → 下载源」和「失败时回退官方源」。自定义源需要支持以下 BMCLAPI 兼容映射：

| 官方地址 | 镜像路径 |
| --- | --- |
| `https://meta.fabricmc.net/*` | `/fabric-meta/*` |
| `https://maven.fabricmc.net/*` | `/maven/*` |
| `https://maven.minecraftforge.net/*` | `/maven/*` |
| `https://maven.neoforged.net/releases/*` | `/maven/*` |

需同时支持 Maven XML 列表、`.sha1` 文件和 jar/zip 下载；Mojang 资源仍使用既有 `/mc/game`、`/v1`、`/assets` 与 `/maven` 规则。镜像缺少某项能力时，可启用官方回退或切换官方源。

## CLI

```powershell
cargo run --bin nscl-cli -- loaders 1.20.1 --loader forge --limit 10
cargo run --bin nscl-cli -- install 1.20.1 --loader forge --loader-version 47.4.28
cargo run --bin nscl-cli -- install 1.20.1 --loader fabric
cargo run --bin nscl-cli -- install 26.3 --loader neoforge --loader-version 26.3.0.95
cargo run --bin nscl-cli -- installed
cargo run --bin nscl-cli -- inspect 1.20.1-forge-47.4.28 --offline
cargo run --bin nscl-cli -- launch 1.20.1-forge-47.4.28
```

未指定加载器版本时，从兼容列表中选择最新稳定版本，没有稳定版本时选择最新预览版本。版本号仅作示例，以实际官方列表为准。支持原有 `--source`、`--mirror`、`--no-fallback`、`--java`、`--game-dir` 参数。

## 协议参考

- [Fabric Meta V2 API](https://github.com/FabricMC/fabric-meta/blob/master/README.md)
- [Forge 安装清单与处理器格式](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/json/Install.java)
- [Forge 官方 Maven](https://maven.minecraftforge.net/net/minecraftforge/forge/)
- [NeoForge 官方 Maven](https://maven.neoforged.net/releases/net/neoforged/neoforge/)
- [BMCLAPI 接口与镜像规则](https://bmclapidoc.bangbang93.com/)

自动测试使用本地模拟服务验证配置合并、镜像、损坏修复、存档保留和失败不发布；官方包安装与实际桌面游戏启动另做人工环境验证。安装器校验不等于覆盖全部历史版本、第三方 Mod 组合或操作系统。

Forge `26.3 / 66.1.0` 已使用官方安装包和现有缓存验证安装完成，覆盖 `MC_UNPACKED` 未被客户端步骤生成的情况；原版和实际补丁 JAR 的 SHA-1 均匹配安装清单。

本次 Windows 环境已验证 Forge `1.20.1-47.4.28`、Fabric `1.20.1 / 0.20.1` 与 NeoForge `26.3 / 26.3.0.95` 的真实安装，三者均已通过启动器的共享启动核心启动并正常退出。Fabric 在 Java 17、含空格的游戏目录和没有映射缓存的新实例中完成了首次映射，生成 `client-intermediary.jar`，随后完成窗口与材质初始化，退出码为 `0`。此前的首次映射失败已定位为沙箱开发目录中的 EXE 继承低完整性标签；复制同一可执行文件到普通用户目录运行后通过。该验证不代表覆盖所有 Minecraft / Fabric 版本、模组组合或操作系统。
