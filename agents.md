# Git 操作约定

- AI 运行容器内的 Git 仓库是用户系统 Git 仓库的只读挂载。
- AI 可以在容器内执行只读 Git 操作，例如查看状态、日志、分支和差异。
- 当用户要求执行任何需要写入 Git 仓库的操作时，AI 不在容器内执行该操作。
- 对于需要写入的 Git 操作，AI 直接向用户提供对应的 PowerShell 格式命令，由用户在其系统中执行。
- 需要写入的 Git 操作包括但不限于：`add`、`commit`、`checkout`、`switch`、`merge`、`rebase`、`reset`、`stash`、`tag`、`branch` 的创建或删除，以及 `push`、`pull` 等会修改本地或远程 Git 状态的操作。
