import { createBot } from 'mineflayer';
import { Command } from 'commander';

new Command()
    .description('棒立ちするだけのMinecraft Botを起動する. Kickされるとプロセスが終了する')
    .showHelpAfterError()
    .argument('<host>', 'ホスト名またはIPアドレス')
    .argument('<port>', 'ポート番号')
    .argument('<username>', 'ユーザー名')
    .argument('<version>', 'Minecraftのバージョン')
    .action((host, port, username, version) => new Promise<void>((resolve) => {
        const bot = createBot({ host, port, username, version, });
        bot.once('spawn', () => {
            console.log('Spawned!');
        });
        bot.once("kicked", () => resolve())
    }))
