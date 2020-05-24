import { ScriptController } from '../src/script/script.controller';

describe('ScriptController', () => {
  it('GET /my-test-key.sh', async () => {
    const controller = new ScriptController();

    const result = await controller.getScript('my-test-key.sh');

    expect(result).toContain('=== DEBUGMYPIPELINE SHELL SCRIPT ===');
    expect(result).toContain('my-test-key');
  });

  it('GET /my-test-key.cmd', async () => {
    const controller = new ScriptController();

    const result = await controller.getScript('my-test-key.cmd');

    expect(result).toContain('=== DEBUGMYPIPELINE CMD SCRIPT ===');
    expect(result).toContain('my-test-key');
  });

  it('GET /my-test-key.invalid', async () => {
    const controller = new ScriptController();

    await expect(controller.getScript('my-test-key.invalid')).rejects.toThrowError();
  });

  it('GET /some-$#-invalid-@-key.sh', async () => {
    const controller = new ScriptController();

    await expect(controller.getScript('some-$#-invalid-@-key.sh')).rejects.toThrowError();
  });
});
