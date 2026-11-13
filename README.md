# ADHD，闭嘴！

> 这段时间我说了算。先做眼前这一件，ADHD，等会儿再吵。

「ADHD，闭嘴！」是一款 macOS 专注计时器，用 Rust、AppKit 和系统 WebKit 编写。

- **专注计时**支持 15、30 分钟和自定时长。右侧菜单栏可以快速开始 30 分钟专注，也可以暂停、继续或停止。专注结束后可以选择休息。
- **白噪音**可选「阴天小雨」「咖啡店」「图书馆」和「壁炉边」。内置音源在专注计时运行时循环播放。[音源素材](assets/audio/SOURCES.md)
- **待办和记录**支持为待办设置截止时间并关联专注。可以按天或按待办查看完成次数和时长，提前结束的专注不计入。
- **奖励**可以给想买的东西设置累计专注时长目标，达成后标记领取。

## 构建与安装

需要 macOS 14 或以上、Rust 1.89 或以上和 Xcode Command Line Tools。克隆仓库后运行以下命令，将应用安装到用户的 Applications 文件夹。

~~~sh
git clone https://github.com/moss0428/adhd-shut-up.git
cd adhd-shut-up
./scripts/package.sh --install
open "$HOME/Applications/ADHD，闭嘴！.app"
~~~

运行 `./scripts/package.sh` 可以在 `dist/` 生成本机签名的应用包。用 `cargo test --locked` 运行测试。应用的版本号可从菜单中的「关于 ADHD，闭嘴！」查看。

## 本地数据

待办、专注记录、奖励和设置保存在 `~/Library/Application Support/水滴/state.json`。白噪音随应用提供，可以离线播放。Mac 睡眠或退出应用时，进行中的计时会暂停，下次打开可以继续。

## 反馈与许可

欢迎在 [Issues](https://github.com/moss0428/adhd-shut-up/issues) 报告问题或提出建议。报告问题时，请附上 App 版本、macOS 版本和复现步骤。

项目代码、文档和自制图标采用 [MIT 许可](LICENSE)。第三方录音及音乐音色沿用各自的许可，详见 [音源来源与许可](assets/audio/SOURCES.md)。
