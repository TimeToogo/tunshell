import { SessionController } from '../src/session/session.controller';

describe('SessionController', () => {
  it('POST /session', async () => {
    const mockModel = { create: jest.fn() };

    const mockKeyGenerator = {
      generate: jest
        .fn()
        .mockReturnValueOnce('key-1')
        .mockReturnValueOnce('key-2'),
    };

    const mockNow = new Date();
    const mockClock = {
      now: jest.fn().mockReturnValueOnce(mockNow),
    };

    const controller = new SessionController(
      mockModel as any,
      mockKeyGenerator,
      mockClock,
    );

    const result = await controller.createSession();

    expect(result.hostKey).toBe('key-1');
    expect(result.clientKey).toBe('key-2');

    expect(mockModel.create.mock.calls.length).toBe(1);
    expect(mockModel.create.mock.calls[0][0]).toStrictEqual({
      host: {
        key: 'key-1',
        joined: false,
        ipAddress: null,
      },
      client: {
        key: 'key-2',
        joined: false,
        ipAddress: null,
      },
      createdAt: mockNow,
    });
  });
});
