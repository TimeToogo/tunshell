import { Controller, Get, Param, BadRequestException } from '@nestjs/common';
import * as fs from 'fs';

const SCRIPTS = {
  sh: fs.readFileSync(process.cwd() + `/static/init-client.sh`).toString('utf8'),
  cmd: fs.readFileSync(process.cwd() + `/static/init-client.cmd`).toString('utf8'),
};

@Controller({ host: 'lets1.debugmypipeline.com' })
export class ScriptController {
  @Get('/:key')
  async getScript(@Param('key') keyWithExt: string): Promise<string> {
    const [key, extension] = keyWithExt.split('.');

    if (!key || !/^[a-zA-Z0-9\-]+$/.test(key)) {
      throw new BadRequestException({ message: 'Invalid key' });
    }

    if (extension !== 'sh' && extension !== 'cmd') {
      throw new BadRequestException({
        message: 'Invalid extension',
      });
    }

    const script = SCRIPTS[extension].replace('__KEY__', key);

    return script;
  }
}
