<p align="center"><a href="README.md">English</a> | <a href="README-ja.md">日本語</a> | <b>简体中文</b></p>

<p align="center">
  <img src="asset/ibisOffice_svg_logo_for_light.svg" alt="ibisOffice" width="480">
</p>

<p align="center">
  <b>用 Rust 和 GPU 打造的新一代办公套件，打开文档时的显示效果与 Microsoft Office 完全一致。</b><br>
  Word (.docx)、Excel (.xlsx) 和 PowerPoint (.pptx)——快速、准确、随处可用。
</p>

---

<p align="center">
  <img src="asset/screenshots.png" alt="在 ibisOffice 中打开的 Word、Excel 和 PowerPoint 文档" width="100%">
</p>

## ibisOffice 是什么？

ibisOffice 是一款用于查看、编辑和保存 Word、Excel 和 PowerPoint 文档的办公套件。它完全用 Rust 编写，所有内容都在 GPU 上绘制。同一套代码既可作为桌面应用运行在 macOS、Windows 和 Linux 上，也可以在网页浏览器中运行。

我们的目标很简单：在 Microsoft Office 中制作的文档，在 ibisOffice 中打开时看起来应该完全一样。换行、分页、表格边框，甚至图表的刻度线——每一个细节，我们都力求得到与 Office 相同的结果。

## 开始使用

### 下载桌面应用

<table>
  <tr>
    <td align="center">
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.7/ibisOffice-0.8.7-Windows.zip">
        <img src="asset/download-windows.svg" alt="Download for Windows" height="54">
      </a><br>
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.7/ibisOffice-0.8.7-Windows.zip"><code>ibisOffice-0.8.7-Windows.zip</code></a><br>
      <small>Windows 10 / 11（64 位）</small>
    </td>
    <td align="center">
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.7/ibisOffice-0.8.7-macOS.zip">
        <img src="asset/download-mac.svg" alt="Download for Mac" height="54">
      </a><br>
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.7/ibisOffice-0.8.7-macOS.zip"><code>ibisOffice-0.8.7-macOS.zip</code></a><br>
      <small>macOS 11 或更高版本（Apple 芯片）</small>
    </td>
    <td align="center">
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.7/ibisOffice-0.8.7-Linux.AppImage">
        <img src="asset/download-linux.svg" alt="Download for Linux" height="54">
      </a><br>
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.7/ibisOffice-0.8.7-Linux.AppImage"><code>ibisOffice-0.8.7-Linux.AppImage</code></a><br>
      <small>Linux x86_64</small>
    </td>
  </tr>
</table>

- **Windows**：下载并解压 `ibisOffice-0.8.7-Windows.zip`，然后运行 `ibisOffice` 文件夹中的 `ibisOffice.exe`。
- **macOS**：下载并解压 `ibisOffice-0.8.7-macOS.zip`，然后将 `ibisOffice.app` 移动到“应用程序”文件夹。
- **Linux**：下载 `ibisOffice-0.8.7-Linux.AppImage`，赋予其可执行权限（`chmod +x`），然后双击运行。可在主流桌面 Linux 发行版上运行（glibc 2.31 或更高版本：Ubuntu、Debian、Fedora、Arch Linux 等）。无需安装，也不需要 FUSE。
- **试用网页版**：在浏览器中打开 [playground](https://ibisjp.github.io/ibis-office/)——无需安装（需要支持 WebGPU 的浏览器）。可以直接打开示例 Word、Excel 和 PowerPoint 文档立即试用。
- **更新日志**：发行说明和近期更新请见 [CHANGELOG.md](CHANGELOG.md)。
- **观看视频**：ibisOffice 的介绍视频（YouTube）。

  [![ibisOffice 介绍视频](asset/video-en.jpg)](https://www.youtube.com/watch?v=nk2BRzMC2vw)

- **命令行界面**：用于批量无界面转换为 PDF/PNG、打印设置和性能基准测试，请见[命令行选项](docs/cli-options.md)（英文）。
- **嵌入网页版**：可选的“关闭”和“下载”按钮会向嵌入页面发送事件，请见[嵌入](docs/web-embedding.md)（英文）。

## 目标

ibisOffice 的目标是成为世界一流的开源办公套件。

- **速度与稳健**：内存安全的 Rust 代码，能够快速打开大型文档，并且不会崩溃。
- **与 Microsoft Office 外观一致**：文字位置、换行、分页、表格、图表和形状的绘制方式与 Office 完全相同。
- **文件依然是 Microsoft Office 文件**：打开并保存后，再在 Office 中重新打开——内容和版式都不会丢失。
- **任何操作系统**：桌面应用在 macOS、Windows 和 Linux 上的运行方式和外观完全相同。
- **浏览器中的原生速度**：借助 WebAssembly 和 WebGPU，无需安装。
- **实时协作**：使用服务器版，多人可以同时编辑同一份文档。

## 特色

### 三种格式，一个应用

- **文档（.docx）**：段落与样式、项目符号与编号、表格、分栏、竖排文字、注音（ruby）与东亚排版、脚注与尾注、页眉与页脚、域（页码、目录、索引、交叉引用）、公式（OMML、LaTeX、UnicodeMath）、形状、图片、图表、SmartArt、墨迹批注、修订与批注、邮件合并、内容控件、文档保护，以及格式转换（.doc、.rtf、.odt、.html、.txt、.pdf）。
- **电子表格（.xlsx）**：500 多个工作表函数、动态数组与溢出、`LET` 和 `LAMBDA`、结构化引用与三维引用、条件格式（数据条、色阶、图标集）、数字格式（包括日本年号和自定义格式）、单元格内的富文本和注音（振假名）、自动筛选、多列排序、表格、数据透视表和数据透视图、切片器和日程表、二维/三维图表以及 Office 2016 以后的图表（瀑布图、树状图、旭日图、直方图、帕累托图、箱形图、漏斗图）、迷你图、表单控件、墨迹批注、数据验证、快速填充、合并计算、分类汇总、模拟分析（单变量求解、数据表、方案管理器、预测工作表、规划求解）、Power Query 数据表、线程批注和备注、工作表和工作簿保护，以及格式转换（.xls、.ods、.csv、.tsv、.txt、.html、.dif、.slk、.pdf）。
- **演示文稿（.pptx）**：幻灯片母版、版式、主题和变体、自定义幻灯片大小、节、页眉和页脚、演讲者备注和讲义；形状（ECMA-376 的全部 187 种预设几何形状、布尔合并运算、顶点编辑、三维旋转、棱台和拉伸）、艺术字和文字效果、公式、表格、图表、SmartArt、图片（裁剪为形状、艺术效果、删除背景）、三维模型、视频和音频（剪裁、淡入淡出、书签、隐藏式字幕）、墨迹批注、动作按钮和缩放链接；动画（进入、强调、退出、动作路径、触发器、图表/SmartArt 的逐步生成）、幻灯片切换（包括平滑切换和音效）、幻灯片放映和演示者视图（双显示器、提词器、激光笔和画笔批注）、自定义放映、排练和录制预览、批注、演示文稿比较、辅助功能检查器、密码加密，以及格式转换/导出（.ppt、.odp、.ppsx、.potx、.pdf、.png、动画 .gif）。
- **启用宏的文件**（.docm、.xlsm、.pptm）也可以打开和保存。宏不会被运行，只会原样保留。

### 兼容性靠测量，而不是靠目测

我们通过测量来验证视觉兼容性。针对每一项功能，我们都制作测试文档，并将真正的 Word、Excel 和 PowerPoint 生成的 PDF 与 ibisOffice 绘制的结果逐像素进行比较。同时检查 ibisOffice 保存的文件能否在 Office 中无需修复直接打开。

### 在 GPU 上绘制的快速界面

界面在 GPU 上绘制（macOS 使用 Metal，Windows 使用 DirectX，Linux 使用 Vulkan，浏览器中使用 WebGPU），因此滚动、缩放和切换工作表都很流畅。用户界面基于 GPUI 构建，GPUI 是为高性能的 Zed 编辑器开发的 UI 框架。

### 桌面与浏览器

同一套 Rust 代码既可作为原生应用运行，也可以编译为 WebAssembly 在浏览器中运行。

## 优势

- **用 Rust 从零开始的全新实现**：不继承旧办公套件的遗留代码。内存安全、并行、快速。
- **通过流程保证渲染兼容性**：超过 70,000 项测试持续地将 ibisOffice 与 Office 的输出逐像素比较。
- **每一次编辑都是一条“命令”**：所有修改都通过只包含 ID 和最终值的命令进行。它既是撤销和重做的单位，也是协作编辑时与服务器交换的单位。
- **忠实于标准**：按照 Office Open XML 的国际标准（ECMA-376 / ISO/IEC 29500 / JIS X 4191）读写文件。形状轮廓等内置定义，直接使用 Ecma 发布的官方 ECMA-376 数据，未作修改。
- **只使用可安全商用的组件**：我们依赖的所有库，都采用允许商用的许可证，例如 Apache-2.0、MIT 或 BSD。不使用 copyleft 库（每次修改都用 `cargo deny` 检查）。

## 开发初衷

我们提供 ibisStorage，一项类似 Google Drive 的文件存储和共享服务。它需要在线预览和编辑 Office 文件——速度快、准确，并且不给服务器增加负担。这就是我们开发 ibisOffice 的原因。

## 速度

越小越快。括号中的数字表示每个产品所花时间是 ibisOffice 的多少倍。

### 从打开文件到显示第一屏

在应用已经运行的状态下，从发出打开文件的指令，到第一页（pptx 为第一张幻灯片，xlsx 为第一个工作表的第一屏）完全显示所需的时间。

<p align="center">
  <img src="asset/benchmark-open-en.svg" alt="Opening Speed Benchmark: ibisOffice vs Microsoft Office, LibreOffice, ONLYOFFICE" width="100%">
</p>

| 文件 | ibisOffice | Microsoft Office | LibreOffice | ONLYOFFICE |
|---|---:|---:|---:|---:|
| 演示文稿（.pptx）<br>31 张幻灯片，35.9 MB | **0.59 秒** | 2.16 秒（3.7×） | 4.40 秒（7.5×） | 4.75 秒（8.1×） |
| 文档（.docx）<br>140–154 页英文，0.12 MB | **0.45 秒** | 1.11 秒（2.5×） | 1.70 秒（3.8×） | 3.15 秒（7.0×） |
| 电子表格（.xlsx）<br>22 个工作表，29 个图表，589,000 个公式，13.7 MB | **0.52 秒** | 14.07 秒（27.0×） | 6.82 秒（13.1×） | 9.36 秒（17.9×） |

我们录制了应用窗口（最高每秒 60 帧），并根据视频测量从发出打开请求到画面不再变化的时间。最后一帧由人工目测确认。

### 100 万行 CSV：首次显示时间

在应用已经运行的状态下，从发出打开 100 万行 CSV 文件的指令，到首屏显示完毕并可进行查看、滚动和缩放操作所需的时间。

<p align="center">
  <img src="asset/benchmark-csv-en.svg" alt="1,000,000-Row CSV First View Speed Benchmark: ibisOffice vs Microsoft Excel, LibreOffice, ONLYOFFICE" width="100%">
</p>

| 文件 | ibisOffice | LibreOffice | Microsoft Excel | ONLYOFFICE |
|---|---:|---:|---:|---:|
| 大型 CSV（.csv）<br>1,048,576 行，20 列，116 MB<br>*（首次显示时间）* | **0.78 秒** | 11.30 秒（14.5×） | 19.04 秒（24.4×） | 137.3 秒*（176.0×） |

\* ONLYOFFICE 未能打开 100 万行的文件（5 分钟后超时）；该数字按其 30 万行加载时间（41.60 秒）的 3.3 倍估算。

### 转换为 PDF

从启动命令行转换，到 PDF 完全写出所需的时间。

<p align="center">
  <img src="asset/benchmark-pdf-en.svg" alt="PDF Export Speed Benchmark: ibisOffice vs Microsoft Office, LibreOffice, ONLYOFFICE" width="100%">
</p>

| 文件 | ibisOffice | Microsoft Office | LibreOffice | ONLYOFFICE |
|---|---:|---:|---:|---:|
| 演示文稿（.pptx）<br>31 张幻灯片，35.9 MB | **1.42 秒** | 8.40 秒（5.9×） | 32.26 秒（22.8×） | 5.87 秒（4.1×） |
| 文档（.docx）<br>140–154 页英文，0.12 MB | **0.69 秒** | 12.49 秒（18.1×） | 3.45 秒（5.0×） | 4.19 秒（6.1×） |
| 电子表格（.xlsx）<br>22 个工作表，29 个图表，589,000 个公式，13.7 MB | **7.26 秒** | 42.94 秒（5.9×） | 19.46 秒（2.7×） | 24.89 秒（3.4×） |

转换方法：ibisOffice 使用 `ibisOffice --export pdf`；Microsoft Office 通过 AppleScript 打开并另存为 PDF；LibreOffice 使用 `soffice --headless --convert-to pdf`；ONLYOFFICE 使用应用自带的 x2t 转换器。

### 测试条件

- 环境：Apple M2 Pro，16 GB 内存，macOS 26.5.1。版本：ibisOffice 0.1.0（开发版本），Microsoft Office 16.113.2，LibreOffice 26.8.0.3，ONLYOFFICE 9.4.0。测量日期为 2026 年 9 月 29 日至 30 日。
- 每个数字均为单次运行的结果。每次运行开始时，应用的所有进程均已退出。
- 所用文件由虚构数据和已去除元数据的公开文档制作而成，发布在 `test/benchmark/` 中。

## 使用场景

### 桌面应用（macOS、Windows、Linux）

- **日常办公软件**：创建和编辑 Word、Excel 和 PowerPoint 文档，并以与 Office 相同的格式保存。
- **即时查看收到的文档**：无需等待即可打开邮件或聊天中的文件，并看到与在 Office 中制作时完全一致的效果。大型工作簿和长篇演示文稿同样打开很快。
- **导出为 PDF 和图片**：无需打开窗口，直接在命令行导出为 PDF 或 PNG——适合在服务器上批量转换和自动处理。

### 网页版（WebAssembly + WebGPU）

- **无需安装即可使用**：只要在浏览器中打开，就能以与桌面应用相同的界面和相同的渲染效果查看和编辑文档。适用于无法安装软件的电脑和共用电脑。
- **在 Web 系统中嵌入文档查看和编辑**：在业务系统、SaaS 产品和门户网站中，就地查看和编辑附带的 Word、Excel 和 PowerPoint 文档，显示效果与在 Office 中一致。
- **无需把文档发送到服务器**：渲染和编辑都在浏览器内完成，因此打开本地文件时，其内容不会发送到任何外部服务器。

## 构建

```bash
git clone --recurse-submodules https://github.com/ibisjp/ibis-office.git
cd ibis-office
cargo build --release
```

- 在 Windows 上，使用 `cargo build --release -p ibis-office-app` 构建（整个工作区包含一个仅限 Linux 的 crate）；应用位于 `target\release\ibisOffice.exe`。
- 在 macOS 上，`scripts/bundle-macos.sh --release` 会在 `target/release/ibisOffice.app` 构建应用。
- 在 Linux 上，`scripts/bundle-linux.sh --release` 会在 `target/dist/ibisOffice-<version>-Linux.AppImage` 构建 AppImage，`scripts/bundle-flatpak.sh --release` 会在 `target/dist/ibisOffice-<version>-Linux-<arch>.flatpak` 构建 Flatpak 包（需要 `flatpak-builder`）。
- 构建浏览器版：使用 `scripts/build-web.sh --release` 构建，并托管 `www/`（支持 WebGPU 的浏览器：Chrome、Edge、Safari 26 或更高版本）。

## 已实现的功能

ibisOffice 为 Word (.docx)、Excel (.xlsx) 和 PowerPoint (.pptx) 文档实现了全面的功能，并严格遵循 ECMA-376 规范。

三种格式已实现功能的完整列表（1,800 多项）请见（英文）：

<p align="center">
  👉 <b><a href="docs/implemented-feature.md">Implemented Features</a></b>
</p>

## 许可证

ibisOffice 采用 **AGPL-3.0-only 和商业许可证双重许可**。

- **AGPL-3.0**：可作为开源软件免费使用、修改和分发。如果您将修改后的版本作为网络服务提供，则也必须按 AGPL-3.0 公开其源代码。请见 [LICENSE](LICENSE)。
- **商业许可证**：如果您要把 ibisOffice 集成到无法满足 AGPL-3.0 条款（公开源代码）的产品或服务中，请使用商业许可证。请见 [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md)。

ibisOffice 的全部著作权归 ibis inc. 所有，因此才能以两种许可证提供。

我们使用的组件，以及其许可证要求的声明，列在 [THIRD-PARTY-NOTICES.md](docs/THIRD-PARTY-NOTICES.md) 中（各个构建的声明：[www/license.md](www/license.md)、[crates/ibis-office-app/macos/license.md](crates/ibis-office-app/macos/license.md)、[crates/ibis-office-app/linux/license.md](crates/ibis-office-app/linux/license.md)）。我们修改过的 GPUI（Apache-2.0）分支发布为 [gpui-pre](https://github.com/ibisjp/gpui-pre)、[gpui-pre-macos](https://github.com/ibisjp/gpui-pre-macos)、[gpui-pre-windows](https://github.com/ibisjp/gpui-pre-windows)、[gpui-pre-linux](https://github.com/ibisjp/gpui-pre-linux)、[gpui-pre-wgpu](https://github.com/ibisjp/gpui-pre-wgpu) 和 [gpui-pre-web](https://github.com/ibisjp/gpui-pre-web)。

## 参与贡献

欢迎在 Issues 中提交缺陷报告、需求和修复建议（目前不接受 Pull Request）。对于涉及代码的 Issue，只需用一句话回复我们的 [CLA](docs/CLA.md)，我们就可以使用它；请见 [CONTRIBUTING.md](docs/CONTRIBUTING.md)（英文）。安全问题请通过 [SECURITY.md](docs/SECURITY.md) 中的渠道报告，不要发在 Issues 中。

## 商标

Microsoft、Microsoft Office、Microsoft 365、Word、Excel 和 PowerPoint 是 Microsoft Corporation 在美国和其他国家/地区的注册商标或商标。LibreOffice 是 The Document Foundation 的注册商标或商标，ONLYOFFICE 是 Ascensio System SIA 的注册商标或商标。所有其他公司和产品名称均为其各自所有者的注册商标或商标。ibisOffice 与上述任何公司均无关联，也未获得其认可或赞助。

ibisOffice 的名称和标志是 ibis inc. 的商标。使用方法请见 [TRADEMARK.md](docs/TRADEMARK.md)（英文）。任何基于 ibisOffice 的产品，都必须显示“Powered by ibisOffice”。

---

© ibis inc.
