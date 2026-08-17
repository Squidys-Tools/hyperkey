namespace Hyperkey.App.Services;

public sealed class SingleInstanceService : IDisposable
{
    private const string MutexName = @"Local\Hyperkey.SingleInstance";
    private Mutex? _mutex;
    private bool _ownsMutex;

    public bool TryAcquire()
    {
        if (_mutex is not null)
        {
            return _ownsMutex;
        }

        _mutex = new Mutex(initiallyOwned: false, MutexName);
        try
        {
            _ownsMutex = _mutex.WaitOne(TimeSpan.Zero);
        }
        catch (AbandonedMutexException)
        {
            // An abandoned mutex means the previous instance exited unexpectedly.
            // WaitOne still transfers ownership to this instance.
            _ownsMutex = true;
        }

        if (!_ownsMutex)
        {
            _mutex.Dispose();
            _mutex = null;
        }

        return _ownsMutex;
    }

    public void Dispose()
    {
        if (_mutex is null)
        {
            return;
        }

        if (_ownsMutex)
        {
            _mutex.ReleaseMutex();
        }

        _mutex.Dispose();
        _mutex = null;
        _ownsMutex = false;
    }
}
